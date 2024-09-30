#![cfg(feature = "spade")]
use std::{collections::HashMap, sync::mpsc::Sender};

use camino::{Utf8Path, Utf8PathBuf};
use ecolor::Color32;
use log::{error, info, warn};
use num::ToPrimitive;
use serde::Deserialize;
use spade::compiler_state::CompilerState;

use color_eyre::{
    eyre::{anyhow, bail, Context, ContextCompat},
    Result,
};
use spade_common::{
    location_info::WithLocation,
    name::{Identifier, NameID, Path},
};
use spade_hir_lowering::MirLowerable;
use spade_types::{ConcreteType, PrimitiveType};
use surfer_translation_types::{
    SubFieldTranslationResult, TranslationResult, Translator, ValueRepr,
};

use crate::wave_container::{ScopeId, VarId, VariableRefExt};
use crate::{message::Message, wasm_util::perform_work, wave_container::VariableMeta, WaveSource};

use super::{TranslationPreference, ValueKind, VariableInfo, VariableValue};

/// Same as the swim::SurferInfo struct
#[derive(Deserialize, Clone)]
pub struct SpadeTestInfo {
    state_file: Utf8PathBuf,
    top_names: HashMap<Utf8PathBuf, String>,
}

pub struct SpadeTranslator {
    state: CompilerState,
    top: NameID,
    top_name: String,
    state_file: Option<Utf8PathBuf>,
}

impl SpadeTranslator {
    pub fn new(top_name: &str, state_file: &Utf8Path) -> Result<Self> {
        let file_content = std::fs::read_to_string(state_file)
            .with_context(|| format!("Failed to read {state_file}"))?;

        Self::new_from_string(top_name, &file_content, Some(state_file.to_path_buf()))
            .context("When loading Spade state from {state_file}")
    }

    pub fn new_from_string(
        top_name: &str,
        state_content: &str,
        state_file: Option<Utf8PathBuf>,
    ) -> Result<Self> {
        let mut opt = ron::Options::default();
        opt.recursion_limit = None;

        let mut de = ron::Deserializer::from_str_with_options(state_content, opt)
            .context("Failed to initialize ron deserializer")?;
        let de = serde_stacker::Deserializer::new(&mut de);
        let state = CompilerState::deserialize(de)
            .with_context(|| format!("Failed to decode Spade state"))?;

        let path = top_name
            .split("::")
            .map(|s| Identifier(s.to_string()).nowhere());
        let (top, _) = state
            .symtab
            .symtab()
            .lookup_unit(&Path(path.collect()).nowhere())
            .map_err(|_| anyhow!("Did not find a unit {top_name} in Spade state"))?;

        Ok(Self {
            state,
            top,
            top_name: top_name.to_string(),
            state_file,
        })
    }

    pub fn init(top_name: &str, state: &str, sender: Sender<Message>) {
        let top_name = top_name.to_string();
        let state_clone = state.to_string();
        perform_work(move || {
            let t = SpadeTranslator::new_from_string(&top_name, &state_clone, None);
            match t {
                Ok(result) => sender
                    .send(Message::TranslatorLoaded(Box::new(result)))
                    .unwrap(),
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });
    }

    pub fn load(
        waves: &Option<WaveSource>,
        top_name_cli: &Option<String>,
        state_file_cli: &Option<Utf8PathBuf>,
        sender: Sender<Message>,
    ) {
        let spade_info = match (waves, top_name_cli, state_file_cli) {
            (_, Some(top), Some(state)) => Some((top.clone(), state.clone())),
            (_, Some(_), None) => {
                warn!(
                    "spade-top was specified on the command line but spade-state was not. Ignoring"
                );
                None
            }
            (_, None, Some(_)) => {
                warn!(
                    "spade-state was specified on the command line but spade-top was not. Ignoring"
                );
                None
            }
            // If we have a file but no spade stuff specified on the command line,
            // we'll look for `build/surfer.toml` to find what the Spade context is if
            // it exists
            (Some(WaveSource::File(file)), _, _) => {
                let ron_file = Utf8PathBuf::from("build/surfer.ron");
                if ron_file.exists() {
                    std::fs::read_to_string(&ron_file)
                        .map_err(|e| error!("Spade translator failed to read {ron_file}. {e}"))
                        .ok()
                        .and_then(|content| {
                            ron::from_str::<SpadeTestInfo>(&content)
                                // .map_err().ok() for  the side effect
                                .map_err(|e| error!("Failed to decode {ron_file}. {e}"))
                                .ok()
                        })
                        .and_then(|info| {
                            if let Some(top) = info.top_names.get(file) {
                                Some((top.clone(), info.state_file.clone()))
                            } else {
                                warn!(
                                    "Found no spade info for {file}. Disabling spade translation"
                                );
                                None
                            }
                        })
                } else {
                    info!("Did not find {ron_file} nor --spade-state and --spade-top. Spade translator will not run");
                    None
                }
            }
            _ => None,
        };
        let Some((top_name, state_file)) = spade_info else {
            return;
        };
        perform_work(move || {
            let t = SpadeTranslator::new(&top_name, &state_file);
            match t {
                Ok(result) => sender
                    .send(Message::TranslatorLoaded(Box::new(result)))
                    .unwrap(),
                Err(e) => sender.send(Message::Error(e)).unwrap(),
            }
        });
    }
}

impl Translator<VarId, ScopeId, Message> for SpadeTranslator {
    fn name(&self) -> String {
        "spade".to_string()
    }

    fn translate(
        &self,
        variable: &VariableMeta,
        value: &VariableValue,
    ) -> Result<TranslationResult> {
        let ty = self
            .state
            .type_of_hierarchical_value(&self.top, &variable.var.full_path()[1..])?;

        let val_vcd_raw = match value {
            VariableValue::BigUint(v) => format!("{v:b}"),
            VariableValue::String(v) => v.clone(),
        };
        let mir_ty = ty.to_mir_type();
        let ty_size = mir_ty
            .size()
            .to_usize()
            .context("Type size does not fit in usize")?;
        let extra_bits = if ty_size > val_vcd_raw.len() {
            let extra_count = ty_size - val_vcd_raw.len();
            let extra_value = match val_vcd_raw.chars().next() {
                Some('0') => "0",
                Some('1') => "0",
                Some('x') => "x",
                Some('z') => "z",
                other => bail!("Found non-bit value in vcd ({other:?})"),
            };
            extra_value.repeat(extra_count)
        } else {
            String::new()
        };
        let val_vcd = format!("{extra_bits}{val_vcd_raw}",);
        translate_concrete(&val_vcd, &ty, &mut false)
    }

    fn variable_info(&self, variable: &VariableMeta) -> Result<VariableInfo> {
        let ty = self
            .state
            .type_of_hierarchical_value(&self.top, &variable.var.full_path()[1..])?;

        info_from_concrete(&ty)
    }

    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference> {
        let ty = self
            .state
            .type_of_hierarchical_value(&self.top, &variable.var.full_path()[1..])?;

        match ty {
            ConcreteType::Single {
                base: PrimitiveType::Clock,
                params: _,
            } => Ok(TranslationPreference::Prefer),
            ConcreteType::Single { base: _, params: _ } => Ok(TranslationPreference::No),
            _ => Ok(TranslationPreference::Prefer),
        }
    }

    fn reload(&self, sender: Sender<Message>) {
        // At this point, we have already loaded the spade info on the first load, so can just
        // pass None as the wave source
        Self::load(
            &None,
            &Some(self.top_name.clone()),
            &self.state_file,
            sender,
        );
    }
}

fn not_present_value(ty: &ConcreteType) -> TranslationResult {
    let subfields = match ty {
        ConcreteType::Tuple(inner) => inner
            .iter()
            .enumerate()
            .map(|(i, t)| SubFieldTranslationResult::new(i, not_present_value(t)))
            .collect(),
        ConcreteType::Struct { name: _, members } => members
            .iter()
            .map(|(n, t)| SubFieldTranslationResult::new(n, not_present_value(t)))
            .collect(),
        ConcreteType::Array { inner, size } => (0..(size.to_u64().unwrap()))
            .map(|i| SubFieldTranslationResult::new(i, not_present_value(inner)))
            .collect(),
        ConcreteType::Enum { options } => not_present_enum_options(options),
        ConcreteType::Single { .. } => vec![],
        ConcreteType::Integer(_) => vec![],
        ConcreteType::Backward(inner) | ConcreteType::Wire(inner) => {
            not_present_value(inner).subfields
        }
    };

    TranslationResult {
        val: ValueRepr::NotPresent,
        subfields,
        kind: ValueKind::Normal,
    }
}

fn not_present_enum_fields(
    fields: &[(Identifier, ConcreteType)],
) -> Vec<SubFieldTranslationResult> {
    fields
        .iter()
        .map(|(name, ty)| SubFieldTranslationResult::new(name.0.clone(), not_present_value(ty)))
        .collect()
}

fn not_present_enum_options(
    options: &[(NameID, Vec<(Identifier, ConcreteType)>)],
) -> Vec<SubFieldTranslationResult> {
    options
        .iter()
        .map(|(opt_name, opt_fields)| SubFieldTranslationResult {
            name: opt_name.1.tail().0.clone(),
            result: TranslationResult {
                val: ValueRepr::NotPresent,
                subfields: not_present_enum_fields(opt_fields),
                kind: ValueKind::Normal,
            },
        })
        .collect()
}

fn translate_concrete(
    val: &str,
    ty: &ConcreteType,
    problematic: &mut bool,
) -> Result<TranslationResult> {
    macro_rules! handle_problematic {
        ($kind:expr) => {
            if *problematic {
                ValueKind::Warn
            } else {
                $kind
            }
        };
        () => {
            handle_problematic!(ValueKind::Normal)
        };
    }
    let mir_ty = ty.to_mir_type();
    let result = match ty {
        ConcreteType::Tuple(inner) => {
            let mut subfields = vec![];
            let mut offset = 0;
            for (i, t) in inner.iter().enumerate() {
                let mut local_problematic = false;
                let end = offset
                    + t.to_mir_type()
                        .size()
                        .to_usize()
                        .context(format!("Value is wider than {} bits", usize::MAX))?;
                let new = translate_concrete(&val[offset..end], t, &mut local_problematic)?;
                offset = end;
                subfields.push(SubFieldTranslationResult::new(i, new));
                *problematic |= local_problematic;
            }

            TranslationResult {
                val: ValueRepr::Tuple,
                subfields,
                kind: handle_problematic!(),
            }
        }
        ConcreteType::Struct { name: _, members } => {
            let mut subfields = vec![];
            let mut offset = 0;
            for (n, t) in members.iter() {
                let mut local_problematic = false;
                let end = offset
                    + t.to_mir_type()
                        .size()
                        .to_usize()
                        .context(format!("Value is wider than {} bits", usize::MAX))?;
                let new = translate_concrete(&val[offset..end], t, &mut local_problematic)?;
                *problematic |= local_problematic;
                offset = end;
                subfields.push(SubFieldTranslationResult::new(n.0.clone(), new));
            }

            TranslationResult {
                val: ValueRepr::Tuple,
                subfields,
                kind: handle_problematic!(),
            }
        }
        ConcreteType::Array { inner, size } => {
            let mut subfields = vec![];
            let mut offset = 0;
            for n in 0..size
                .to_usize()
                .context(format!("Array size is greater than {}", usize::MAX))?
            {
                let mut local_problematic = false;
                let end = offset
                    + inner
                        .to_mir_type()
                        .size()
                        .to_usize()
                        .context(format!("Value is wider than {} bits", usize::MAX))?;
                let new = translate_concrete(&val[offset..end], inner, &mut local_problematic)?;
                *problematic |= local_problematic;
                offset = end;
                subfields.push(SubFieldTranslationResult::new(n, new));
            }

            TranslationResult {
                val: ValueRepr::Array,
                subfields,
                kind: handle_problematic!(),
            }
        }
        ConcreteType::Enum { options } => {
            let tag_size = (options.len() as f32).log2().ceil() as usize;
            let tag_section = &val[0..tag_size];
            if tag_section.contains('x') {
                *problematic = true;
                TranslationResult {
                    val: ValueRepr::String(format!("xTAG(0b{tag_section})")),
                    subfields: not_present_enum_options(options),
                    kind: ValueKind::Undef,
                }
            } else if tag_section.contains('z') {
                *problematic = true;
                TranslationResult {
                    val: ValueRepr::String(format!("zTAG(0b{tag_section})")),
                    subfields: not_present_enum_options(options),
                    kind: ValueKind::HighImp,
                }
            } else {
                let tag = usize::from_str_radix(tag_section, 2)
                    .with_context(|| format!("Unexpected characters in enum tag {tag_section}"))?;

                if tag > options.len() {
                    *problematic = true;
                    TranslationResult {
                        val: ValueRepr::String(format!("?TAG(0b{tag_section})")),
                        subfields: not_present_enum_options(options),
                        kind: ValueKind::Undef,
                    }
                } else {
                    let mut kind = ValueKind::Normal;
                    let subfields = options
                        .iter()
                        .enumerate()
                        .map(|(i, (name, fields))| {
                            let name = name.1.tail().0;
                            let mut offset = tag_size;

                            let subfields = fields
                                .iter()
                                .map(|(f_name, f_ty)| {
                                    let mut local_problematic = false;
                                    let end = offset
                                        + f_ty.to_mir_type().size().to_usize().context(format!(
                                            "Value is wider than {} bits",
                                            usize::MAX
                                        ))?;
                                    let new = translate_concrete(
                                        &val[offset..end],
                                        f_ty,
                                        &mut local_problematic,
                                    )?;
                                    offset = end;

                                    *problematic |= local_problematic;

                                    Ok(SubFieldTranslationResult::new(f_name.0.clone(), new))
                                })
                                .collect::<Result<_>>()?;

                            let result = if i == tag {
                                if name == "None" {
                                    kind = ValueKind::Custom(Color32::DARK_GRAY);
                                }

                                SubFieldTranslationResult {
                                    name,
                                    result: TranslationResult {
                                        val: if fields.len() == 1 {
                                            ValueRepr::Tuple
                                        } else {
                                            ValueRepr::Struct
                                        },
                                        subfields,
                                        kind: handle_problematic!(),
                                    },
                                }
                            } else {
                                SubFieldTranslationResult {
                                    name,
                                    result: TranslationResult {
                                        val: ValueRepr::NotPresent,
                                        subfields: not_present_enum_fields(fields),
                                        kind: handle_problematic!(),
                                    },
                                }
                            };
                            Ok(result)
                        })
                        .collect::<Result<_>>()?;

                    TranslationResult {
                        val: ValueRepr::Enum {
                            idx: tag,
                            name: options[tag].0 .1.tail().0.clone(),
                        },
                        kind,
                        subfields,
                    }
                }
            }
        }
        ConcreteType::Single {
            base: PrimitiveType::Bool | PrimitiveType::Clock,
            params: _,
        } => TranslationResult {
            val: ValueRepr::Bit(val.chars().next().unwrap()),
            kind: ValueKind::Normal,
            subfields: vec![],
        },
        ConcreteType::Single { base: _, params: _ } | ConcreteType::Integer(_) => {
            TranslationResult {
                val: ValueRepr::Bits(
                    mir_ty.size().to_u64().context("Size did not fit in u64")?,
                    val.to_string(),
                ),
                kind: ValueKind::Normal,
                subfields: vec![],
            }
        }
        ConcreteType::Backward(_) => TranslationResult {
            val: ValueRepr::String("*backward*".to_string()),
            kind: ValueKind::Custom(Color32::from_gray(128)),
            subfields: vec![],
        },
        ConcreteType::Wire(inner) => translate_concrete(val, inner, problematic)?,
    };
    Ok(result)
}

fn info_from_concrete(ty: &ConcreteType) -> Result<VariableInfo> {
    let result = match ty {
        ConcreteType::Tuple(inner) => VariableInfo::Compound {
            subfields: inner
                .iter()
                .enumerate()
                .map(|(i, inner)| Ok((format!("{i}"), info_from_concrete(inner)?)))
                .collect::<Result<_>>()?,
        },
        ConcreteType::Struct { name: _, members } => VariableInfo::Compound {
            subfields: members
                .iter()
                .map(|(f, inner)| Ok((f.0.clone(), info_from_concrete(inner)?)))
                .collect::<Result<_>>()?,
        },
        ConcreteType::Array { inner, size } => VariableInfo::Compound {
            subfields: (0..size.to_u64().context("Array size did not fit in u64")?)
                .map(|i| Ok((format!("{i}"), info_from_concrete(inner)?)))
                .collect::<Result<_>>()?,
        },
        ConcreteType::Enum { options } => VariableInfo::Compound {
            subfields: options
                .iter()
                .map(|(name, fields)| {
                    Ok((
                        name.1.tail().0.clone(),
                        VariableInfo::Compound {
                            subfields: fields
                                .iter()
                                .map(|(f_name, f_ty)| {
                                    Ok((f_name.0.clone(), info_from_concrete(f_ty)?))
                                })
                                .collect::<Result<_>>()?,
                        },
                    ))
                })
                .collect::<Result<_>>()?,
        },
        ConcreteType::Single {
            base: PrimitiveType::Bool,
            params: _,
        } => VariableInfo::Bool,
        ConcreteType::Single {
            base: PrimitiveType::Clock,
            params: _,
        } => VariableInfo::Clock,
        ConcreteType::Single { .. } => VariableInfo::Bits,
        ConcreteType::Integer(_) => VariableInfo::Bits,
        ConcreteType::Backward(inner) => info_from_concrete(inner)?,
        ConcreteType::Wire(inner) => info_from_concrete(inner)?,
    };
    Ok(result)
}
