use anyhow::anyhow;
use color_eyre::eyre::{eyre, Context, OptionExt};
use color_eyre::Result;
use ecolor::Color32;
use extism_pdk::{error, plugin_fn, FnResult, FromBytes, Msgpack, ToBytes};
use num::ToPrimitive;
use serde::{Deserialize, Serialize};
use spade::compiler_state::CompilerState;
use spade_common::location_info::WithLocation;
use spade_common::name::{Identifier, NameID, Path};
use spade_hir_lowering::MirLowerable;
use spade_types::{ConcreteType, PrimitiveType};
use surfer_translation_types::plugin_types::TranslateParams;
use surfer_translation_types::{
    PluginConfig, SubFieldTranslationResult, TranslationPreference, TranslationResult, ValueKind,
    ValueRepr, VariableInfo, VariableMeta, VariableValue,
};

#[derive(Deserialize, Serialize, ToBytes, FromBytes)]
#[encoding(Msgpack)]
pub struct SpadeTranslator {
    pub state: CompilerState,
    pub top: NameID,
}

#[plugin_fn]
pub fn new(PluginConfig(config): PluginConfig) -> FnResult<()> {
    let mut opt = ron::Options::default();
    opt.recursion_limit = None;

    let mut de = ron::Deserializer::from_str_with_options(&config.get("state").unwrap(), opt)
        .context("Failed to initialize ron deserializer")
        .unwrap();
    let de = serde_stacker::Deserializer::new(&mut de);
    let state = CompilerState::deserialize(de)
        .context("Failed to decode compiler state")
        .unwrap();

    let top_name = config.get("top").unwrap();
    let path = top_name
        .split("::")
        .map(|s| Identifier(s.to_string()).nowhere());
    let (top, _) = state
        .symtab
        .symtab()
        .lookup_unit(&Path(path.collect()).nowhere())
        .map_err(|_| anyhow!("Did not find a unit {top_name} in compiler state"))
        .unwrap();

    let ctx = SpadeTranslator { state, top };
    extism_pdk::var::set("state", &ctx).unwrap();
    Ok(())
}

#[plugin_fn]
pub fn name() -> FnResult<&'static str> {
    Ok("spade (plugin)")
}

#[plugin_fn]
pub fn translate(params: TranslateParams) -> FnResult<TranslationResult> {
    let ctx: SpadeTranslator = extism_pdk::var::get("state").unwrap().unwrap();
    let variable = params.variable;
    let value = params.value;
    let ty = ctx
        .state
        .type_of_hierarchical_value(&ctx.top, &variable.var.full_path()[1..]).unwrap() /* ? */;

    let val_vcd_raw = match value {
        VariableValue::BigUint(v) => format!("{v:b}"),
        VariableValue::String(v) => v.clone(),
    };
    let mir_ty = ty.to_mir_type();
    let ty_size = mir_ty
        .size()
        .to_usize()
        .ok_or_else(|| anyhow!("Type size does not fit in usize"))?;
    let extra_bits = if ty_size > val_vcd_raw.len() {
        let extra_count = ty_size - val_vcd_raw.len();
        let extra_value = match val_vcd_raw.chars().next() {
            Some('0') => "0",
            Some('1') => "0",
            Some('x') => "x",
            Some('z') => "z",
            other => {
                return Err(anyhow!("Found non-bit value in vcd ({other:?})").into());
            }
        };
        extra_value.repeat(extra_count)
    } else {
        String::new()
    };
    let val_vcd = format!("{extra_bits}{val_vcd_raw}");
    let result = translate_concrete(&val_vcd, &ty, &mut false).unwrap();
    Ok(result)
}

#[plugin_fn]
pub fn variable_info(variable: VariableMeta<(), ()>) -> FnResult<VariableInfo> {
    let ctx: SpadeTranslator = extism_pdk::var::get("state").unwrap().unwrap();
    let ty = ctx
        .state
        .type_of_hierarchical_value(&ctx.top, &variable.var.full_path()[1..]).unwrap()/*?*/;

    let result = info_from_concrete(&ty).unwrap();
    Ok(result)
}

#[plugin_fn]
pub fn translates(variable: VariableMeta<(), ()>) -> FnResult<TranslationPreference> {
    let ctx: SpadeTranslator =
        extism_pdk::var::get("state")?.ok_or_else(|| anyhow!("`new` not called"))?;

    let Ok(ty) = ctx
        .state
        .type_of_hierarchical_value(&ctx.top, &variable.var.full_path()[1..])
    else {
        error!(
            "could not find type of value {:?}",
            variable.var.full_path()
        );
        return Ok(TranslationPreference::No);
    };

    match ty {
        ConcreteType::Single {
            base: PrimitiveType::Clock,
            params: _,
        } => Ok(TranslationPreference::Prefer),
        ConcreteType::Single { base: _, params: _ } => Ok(TranslationPreference::No),
        _ => Ok(TranslationPreference::Prefer),
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
                        .ok_or_else(|| eyre!("Value is wider than {} bits", usize::MAX))?;
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
                        .ok_or_else(|| eyre!("Value is wider than {} bits", usize::MAX))?;
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
                .ok_or_else(|| eyre!("Array size is greater than {}", usize::MAX))?
            {
                let mut local_problematic = false;
                let end = offset
                    + inner
                        .to_mir_type()
                        .size()
                        .to_usize()
                        .ok_or_else(|| eyre!("Value is wider than {} bits", usize::MAX))?;
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
                                        + f_ty.to_mir_type().size().to_usize().ok_or_else(
                                            || eyre!("Value is wider than {} bits", usize::MAX),
                                        )?;
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
                                    kind = ValueKind::Custom(Color32::DARK_GRAY)
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
                    mir_ty
                        .size()
                        .to_u64()
                        .ok_or_eyre("Size did not fit in u64")?,
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
            subfields: (0..size.to_u64().ok_or_eyre("Array size did not fit in u64")?)
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
