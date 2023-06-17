use std::collections::HashMap;

use camino::Utf8Path;
use num::ToPrimitive;
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
use spade_types::ConcreteType;
use vcd_translate::translation::{inner_translate_value, value_from_str};

use super::{SignalInfo, TranslationResult, Translator, ValueRepr};

pub struct SpadeTranslator {
    state: CompilerState,
    top: NameID,
}

impl SpadeTranslator {
    pub fn new(top: &str, state_file: &Utf8Path) -> Result<Self> {
        let file_content = std::fs::read_to_string(&state_file)
            .with_context(|| format!("Failed to read {state_file}"))?;

        let state: CompilerState = ron::from_str(&file_content)
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
            let extra_value = match val_vcd_raw.chars().last() {
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
        translate_concrete(&val_vcd, &ty)
    }

    fn signal_info(&self, signal: &fastwave_backend::Signal, _name: &str) -> Result<SignalInfo> {
        let ty = self
            .state
            .type_of_hierarchical_value(&self.top, &signal.path()[1..])?;

        info_from_concrete(&ty)
    }
}

fn translate_concrete(val: &str, ty: &ConcreteType) -> Result<TranslationResult> {
    let mir_ty = ty.to_mir_type();
    let result = match ty {
        ConcreteType::Tuple(inner) => {
            let mut subfields = vec![];
            let mut offset = 0;
            for (i, t) in inner.iter().enumerate() {
                let end = offset
                    + t.to_mir_type()
                        .size()
                        .to_usize()
                        .context(format!("Value is wider than {} bits", usize::MAX))?;
                let new = translate_concrete(&val[offset..end], t)?;
                offset = end;
                subfields.push((format!("{i}"), new));
            }

            TranslationResult {
                val: ValueRepr::Tuple,
                subfields,
                durations: HashMap::new(),
            }
        }
        ConcreteType::Struct { name, members } => {
            let mut subfields = vec![];
            let mut offset = 0;
            for (n, t) in members.iter() {
                let end = offset
                    + t.to_mir_type()
                        .size()
                        .to_usize()
                        .context(format!("Value is wider than {} bits", usize::MAX))?;
                let new = translate_concrete(&val[offset..end], t)?;
                offset = end;
                subfields.push((n.0.clone(), new));
            }

            TranslationResult {
                val: ValueRepr::Tuple,
                subfields,
                durations: HashMap::new(),
            }
        }
        ConcreteType::Array { inner, size } => todo!(),
        ConcreteType::Enum { options } => TranslationResult {
            val: ValueRepr::String(format!("Enums not yet handled")),
            subfields: vec![],
            durations: HashMap::new(),
        },
        ConcreteType::Single { base: _, params: _ } | ConcreteType::Integer(_) => {
            TranslationResult {
                val: ValueRepr::Bits(
                    mir_ty.size().to_u64().context("Size did not fit in u64")?,
                    val.to_string(),
                ),
                subfields: vec![],
                durations: HashMap::new(),
            }
        }
        ConcreteType::Backward(_) => TranslationResult {
            val: ValueRepr::String("*backward*".to_string()),
            subfields: vec![],
            durations: HashMap::new(),
        },
        ConcreteType::Wire(inner) => translate_concrete(val, inner)?,
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
        ConcreteType::Enum { .. } => SignalInfo::Bits,
        ConcreteType::Single { .. } => SignalInfo::Bits,
        ConcreteType::Integer(_) => SignalInfo::Bits,
        ConcreteType::Backward(inner) => info_from_concrete(inner)?,
        ConcreteType::Wire(inner) => info_from_concrete(inner)?,
    };
    Ok(result)
}
