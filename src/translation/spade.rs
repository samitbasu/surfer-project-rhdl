use std::collections::HashMap;

use camino::Utf8Path;
use eframe::epaint::Color32;
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

use super::{
    SignalInfo, TranslationPreference, TranslationResult, Translator, ValueColor, ValueRepr,
};

pub struct SpadeTranslator {
    state: CompilerState,
    top: NameID,
}

impl SpadeTranslator {
    pub fn new(top: &str, state_file: &Utf8Path) -> Result<Self> {
        let file_content = std::fs::read_to_string(&state_file)
            .with_context(|| format!("Failed to read {state_file}"))?;

        let mut opt = ron::Options::default();
        opt.recursion_limit = None;

        let mut de = ron::Deserializer::from_str_with_options(&file_content, opt)
            .context("Failed to initialize ron deserializer")?;
        let de = serde_stacker::Deserializer::new(&mut de);
        let state = CompilerState::deserialize(de)
            .with_context(|| format!("Failed to decode {state_file}"))?;

        let path = top.split("::").map(|s| Identifier(s.to_string()).nowhere());
        let (top, _) = state
            .symtab
            .symtab()
            .lookup_unit(&Path(path.collect()).nowhere())
            .map_err(|_| anyhow!("Did not find a unit {top} in {state_file}"))?;

        Ok(Self { state, top })
    }
}

impl Translator for SpadeTranslator {
    fn name(&self) -> String {
        "spade".to_string()
    }

    fn translate(
        &self,
        signal: &fastwave_backend::Signal,
        value: &fastwave_backend::SignalValue,
    ) -> Result<TranslationResult> {
        let ty = self
            .state
            .type_of_hierarchical_value(&self.top, &signal.path()[1..])?;

        let val_vcd_raw = match value {
            fastwave_backend::SignalValue::BigUint(v) => format!("{v:b}"),
            fastwave_backend::SignalValue::String(v) => v.clone(),
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

    fn signal_info(&self, signal: &fastwave_backend::Signal, _name: &str) -> Result<SignalInfo> {
        let ty = self
            .state
            .type_of_hierarchical_value(&self.top, &signal.path()[1..])?;

        info_from_concrete(&ty)
    }

    fn translates(&self, signal: &fastwave_backend::Signal) -> Result<TranslationPreference> {
        let ty = self
            .state
            .type_of_hierarchical_value(&self.top, &signal.path()[1..])?;

        match ty {
            ConcreteType::Single { base: _, params: _ } => Ok(TranslationPreference::No),
            _ => Ok(TranslationPreference::Prefer),
        }
    }
}

fn not_present_value(ty: &ConcreteType) -> TranslationResult {
    let subfields = match ty {
        ConcreteType::Tuple(inner) => inner
            .iter()
            .enumerate()
            .map(|(i, t)| (format!("{i}"), not_present_value(t)))
            .collect(),
        ConcreteType::Struct { name: _, members } => members
            .iter()
            .map(|(n, t)| (format!("{n}"), not_present_value(t)))
            .collect(),
        ConcreteType::Array { inner, size } => (0..(size.to_u64().unwrap()))
            .map(|i| (format!("{i}"), not_present_value(inner)))
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
        color: ValueColor::Normal,
        durations: HashMap::new(),
    }
}

fn not_present_enum_fields(
    fields: &[(Identifier, ConcreteType)],
) -> Vec<(String, TranslationResult)> {
    fields
        .iter()
        .map(|(name, ty)| (name.0.clone(), not_present_value(ty)))
        .collect()
}

fn not_present_enum_options(
    options: &Vec<(NameID, Vec<(Identifier, ConcreteType)>)>,
) -> Vec<(String, TranslationResult)> {
    options
        .iter()
        .map(|(opt_name, opt_fields)| {
            (
                opt_name.1.tail().0.clone(),
                TranslationResult {
                    val: ValueRepr::NotPresent,
                    subfields: not_present_enum_fields(opt_fields),
                    color: ValueColor::Normal,
                    durations: HashMap::new(),
                },
            )
        })
        .collect()
}

fn translate_concrete(
    val: &str,
    ty: &ConcreteType,
    problematic: &mut bool,
) -> Result<TranslationResult> {
    macro_rules! handle_problematic {
        () => {
            if *problematic {
                ValueColor::Warn
            } else {
                ValueColor::Normal
            }
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
                subfields.push((format!("{i}"), new));
                *problematic |= local_problematic;
            }

            TranslationResult {
                val: ValueRepr::Tuple,
                subfields,
                color: handle_problematic!(),
                durations: HashMap::new(),
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
                subfields.push((n.0.clone(), new));
            }

            TranslationResult {
                val: ValueRepr::Tuple,
                subfields,
                color: handle_problematic!(),
                durations: HashMap::new(),
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
                subfields.push((format!("{n}"), new));
            }

            TranslationResult {
                val: ValueRepr::Array,
                subfields,
                color: handle_problematic!(),
                durations: HashMap::new(),
            }
        }
        ConcreteType::Enum { options } => {
            let tag_size = (options.len() as f32).log2().ceil() as usize;
            let tag_section = &val[0..tag_size];
            if tag_section.contains('x') {
                *problematic = true;
                TranslationResult {
                    val: ValueRepr::String(format!("xTAG(0b{tag_section})")),
                    subfields: not_present_enum_options(&options),
                    color: ValueColor::Undef,
                    durations: HashMap::new(),
                }
            } else if tag_section.contains('z') {
                *problematic = true;
                TranslationResult {
                    val: ValueRepr::String(format!("zTAG(0b{tag_section})")),
                    subfields: not_present_enum_options(&options),
                    color: ValueColor::HighImp,
                    durations: HashMap::new(),
                }
            } else {
                let tag = usize::from_str_radix(tag_section, 2)
                    .with_context(|| format!("Unexpected characters in enum tag {tag_section}"))?;

                if tag > options.len() {
                    *problematic = true;
                    TranslationResult {
                        val: ValueRepr::String(format!("?TAG(0b{tag_section})")),
                        subfields: not_present_enum_options(&options),
                        color: ValueColor::Undef,
                        durations: HashMap::new(),
                    }
                } else {
                    TranslationResult {
                        val: ValueRepr::Enum {
                            idx: tag,
                            name: options[tag].0 .1.tail().0.clone(),
                        },
                        color: ValueColor::Normal,
                        subfields: options
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
                                            + f_ty.to_mir_type().size().to_usize().context(
                                                format!("Value is wider than {} bits", usize::MAX),
                                            )?;
                                        let new = translate_concrete(
                                            &val[offset..end],
                                            f_ty,
                                            &mut local_problematic,
                                        )?;
                                        offset = end;

                                        *problematic |= local_problematic;

                                        Ok((f_name.0.clone(), new))
                                    })
                                    .collect::<Result<_>>()?;

                                let result = if i == tag {
                                    (
                                        name.clone(),
                                        TranslationResult {
                                            val: ValueRepr::Struct,
                                            subfields,
                                            color: handle_problematic!(),
                                            durations: HashMap::new(),
                                        },
                                    )
                                } else {
                                    (
                                        name.clone(),
                                        TranslationResult {
                                            val: ValueRepr::NotPresent,
                                            subfields: not_present_enum_fields(fields),
                                            color: handle_problematic!(),
                                            durations: HashMap::new(),
                                        },
                                    )
                                };
                                Ok(result)
                            })
                            .collect::<Result<_>>()?,
                        durations: HashMap::new(),
                    }
                }
            }
        }
        ConcreteType::Single {
            base: PrimitiveType::Bool | PrimitiveType::Clock,
            params: _,
        } => TranslationResult {
            val: ValueRepr::Bit(val.chars().next().unwrap()),
            color: ValueColor::Normal,
            subfields: vec![],
            durations: HashMap::new(),
        },
        ConcreteType::Single { base: _, params: _ } | ConcreteType::Integer(_) => {
            TranslationResult {
                val: ValueRepr::Bits(
                    mir_ty.size().to_u64().context("Size did not fit in u64")?,
                    val.to_string(),
                ),
                color: ValueColor::Normal,
                subfields: vec![],
                durations: HashMap::new(),
            }
        }
        ConcreteType::Backward(_) => TranslationResult {
            val: ValueRepr::String("*backward*".to_string()),
            color: ValueColor::Custom(Color32::from_gray(128)),
            subfields: vec![],
            durations: HashMap::new(),
        },
        ConcreteType::Wire(inner) => translate_concrete(val, inner, problematic)?,
    };
    Ok(result)
}

fn info_from_concrete(ty: &ConcreteType) -> Result<SignalInfo> {
    let result = match ty {
        ConcreteType::Tuple(inner) => SignalInfo::Compound {
            subfields: inner
                .iter()
                .enumerate()
                .map(|(i, inner)| Ok((format!("{i}"), info_from_concrete(inner)?)))
                .collect::<Result<_>>()?,
        },
        ConcreteType::Struct { name: _, members } => SignalInfo::Compound {
            subfields: members
                .iter()
                .map(|(f, inner)| Ok((f.0.clone(), info_from_concrete(inner)?)))
                .collect::<Result<_>>()?,
        },
        ConcreteType::Array { inner, size } => SignalInfo::Compound {
            subfields: (0..size.to_u64().context("Array size did not fit in u64")?)
                .map(|i| Ok((format!("{i}"), info_from_concrete(inner)?)))
                .collect::<Result<_>>()?,
        },
        ConcreteType::Enum { options } => SignalInfo::Compound {
            subfields: options
                .iter()
                .map(|(name, fields)| {
                    Ok((
                        name.1.tail().0.clone(),
                        SignalInfo::Compound {
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
            base: PrimitiveType::Bool | PrimitiveType::Clock,
            params: _,
        } => SignalInfo::Bool,
        ConcreteType::Single { .. } => SignalInfo::Bits,
        ConcreteType::Integer(_) => SignalInfo::Bits,
        ConcreteType::Backward(inner) => info_from_concrete(inner)?,
        ConcreteType::Wire(inner) => info_from_concrete(inner)?,
    };
    Ok(result)
}
