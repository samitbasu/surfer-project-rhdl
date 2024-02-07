use crate::signal_type::SignalType;
use crate::time::{TimeScale, TimeUnit};
use color_eyre::eyre::bail;
use color_eyre::{eyre::anyhow, Result};
use log::warn;
use num::{BigUint, ToPrimitive};
use wellen::{
    GetItem, Time, TimeTableIdx, Timescale, TimescaleUnit, Var, VarRef, VarType, Waveform,
};

use crate::wave_container::{MetaData, ModuleRef, QueryResult, SignalMeta, SignalRef, SignalValue};

#[derive(Debug)]
pub struct WellenContainer {
    inner: Waveform,
    scopes: Vec<String>,
    vars: Vec<String>,
}

impl WellenContainer {
    pub fn new(inner: Waveform) -> Self {
        // generate a list of names for all variables and scopes since they will be requested by the parser
        let h = inner.hierarchy();
        let scopes = h
            .iter_scopes()
            .map(|r| r.full_name(h).to_string())
            .collect::<Vec<_>>();
        let vars = h
            .iter_vars()
            .map(|r| r.full_name(h).to_string())
            .collect::<Vec<_>>();

        Self {
            inner,
            scopes,
            vars,
        }
    }

    pub fn metadata(&self) -> MetaData {
        let timescale = self
            .inner
            .hierarchy()
            .timescale()
            .unwrap_or(Timescale::new(1, TimescaleUnit::Unknown));
        let date = None;
        MetaData {
            date,
            version: Some(self.inner.hierarchy().version().to_string()),
            timescale: TimeScale {
                unit: TimeUnit::from(timescale.unit),
                multiplier: Some(timescale.factor),
            },
        }
    }

    pub fn max_timestamp(&self) -> Option<BigUint> {
        self.inner.time_table().last().map(|t| BigUint::from(*t))
    }

    pub fn signal_names(&self) -> Vec<String> {
        self.vars.clone()
    }

    pub fn signals_in_module(&self, module: &ModuleRef) -> Vec<SignalRef> {
        let h = self.inner.hierarchy();
        let scope = match h.lookup_scope(&module.0) {
            Some(id) => h.get(id),
            None => {
                warn!("Found no module '{module}'. Defaulting to no signals");
                return vec![];
            }
        };
        scope
            .vars(h)
            .map(|id| SignalRef {
                path: module.clone(),
                name: h.get(id).name(h).to_string(),
            })
            .collect::<Vec<_>>()
    }

    pub fn signal_exists(&self, signal: &SignalRef) -> bool {
        self.get_var_ref(signal).is_ok()
    }

    pub fn get_var(&self, r: &SignalRef) -> Result<&Var> {
        let h = self.inner.hierarchy();
        self.get_var_ref(r).map(|r| h.get(r))
    }

    fn get_var_ref(&self, r: &SignalRef) -> Result<VarRef> {
        let h = self.inner.hierarchy();
        let var = match h.lookup_var(&r.path.0, &r.name) {
            None => bail!("Failed to find signal: {r:?}"),
            Some(id) => id,
        };
        Ok(var)
    }

    pub fn load_signal(&mut self, r: &SignalRef) -> Result<&Var> {
        let var_ref = self.get_var_ref(r)?;
        let signal_ref = self.inner.hierarchy().get(var_ref).signal_ref();
        self.inner.load_signals(&[signal_ref]);
        Ok(self.inner.hierarchy().get(var_ref))
    }

    pub fn load_signals<S: AsRef<SignalRef>, T: Iterator<Item = S>>(
        &mut self,
        signals: T,
    ) -> Result<()> {
        let h = self.inner.hierarchy();
        let signal_refs = signals
            .flat_map(|s| {
                let r = s.as_ref();
                h.lookup_var(&r.path.0, &r.name)
                    .map(|v| h.get(v).signal_ref())
            })
            .collect::<Vec<_>>();
        self.inner.load_signals(&signal_refs);
        Ok(())
    }

    fn time_to_time_table_idx(&self, time: &BigUint) -> TimeTableIdx {
        let time: Time = time.to_u64().expect("unsupported time!");
        // binary search to find correct index
        let idx = binary_search(self.inner.time_table(), time);
        assert!(self.inner.time_table()[idx] <= time);
        idx as TimeTableIdx
    }

    pub fn query_signal(&self, signal: &SignalRef, time: &BigUint) -> Result<QueryResult> {
        let h = self.inner.hierarchy();
        // find variable from string
        let var_ref = h
            .lookup_var(&signal.path.0, &signal.name)
            .ok_or_else(|| anyhow!("Failed to find signal {signal:?}"))?;
        // map variable to signal
        let signal_ref = h.get(var_ref).signal_ref();
        let sig = match self.inner.get_signal(signal_ref) {
            Some(sig) => sig,
            None => bail!("internal error: signal {signal:?} should have been loaded!"),
        };
        // convert time to index
        let idx = self.time_to_time_table_idx(time);
        // calculate time
        let time_table = self.inner.time_table();
        // get data offset
        let offset = sig.get_offset(idx);
        // which time did we actually get the value for?
        let offset_time_idx = sig.get_time_idx_at(&offset);
        let offset_time = time_table[offset_time_idx as usize];
        // get the last value in a time step (since we ignore delta cycles for now)
        let current_value = sig.get_value_at(&offset, offset.elements - 1);
        // the next time the signal changes
        let next_time = offset
            .next_index
            .and_then(|i| time_table.get(i.get() as usize));

        let converted_value = convert_signal_value(current_value);
        let result = QueryResult {
            current: Some((BigUint::from(offset_time), converted_value)),
            next: next_time.map(|t| BigUint::from(*t)),
        };
        Ok(result)
    }

    pub fn module_names(&self) -> Vec<String> {
        self.scopes.clone()
    }

    pub fn root_modules(&self) -> Vec<ModuleRef> {
        let h = self.inner.hierarchy();
        h.scopes()
            .map(|id| ModuleRef(vec![h.get(id).name(h).to_string()]))
            .collect::<Vec<_>>()
    }

    pub fn child_modules(&self, module: &ModuleRef) -> Result<Vec<ModuleRef>> {
        let h = self.inner.hierarchy();
        let scope = match h.lookup_scope(&module.0) {
            Some(id) => h.get(id),
            None => return Err(anyhow!("Failed to find module {module:?}")),
        };
        Ok(scope
            .scopes(h)
            .map(|id| module.with_submodule(h.get(id).name(h).to_string()))
            .collect::<Vec<_>>())
    }

    pub fn module_exists(&self, module: &ModuleRef) -> bool {
        let h = self.inner.hierarchy();
        h.lookup_scope(&module.0).is_some()
    }
}

fn convert_signal_value(value: wellen::SignalValue) -> SignalValue {
    match value {
        wellen::SignalValue::Binary(data, _bits) => {
            SignalValue::BigUint(BigUint::from_bytes_be(data))
        }
        wellen::SignalValue::FourValue(_, _) | wellen::SignalValue::NineValue(_, _) => {
            SignalValue::String(
                value
                    .to_bit_string()
                    .expect("failed to convert value {value:?} to a string"),
            )
        }
        wellen::SignalValue::String(value) => SignalValue::String(value.to_string()),
        wellen::SignalValue::Real(value) => SignalValue::String(format!("{}", value)),
    }
}

pub(crate) fn var_to_meta<'a>(var: &Var, r: &'a SignalRef) -> SignalMeta<'a> {
    SignalMeta {
        sig: r,
        num_bits: var.length(),
        signal_type: Some(var_to_signal_type(var.var_type())),
        index: var.index().map(index_to_string),
    }
}

fn index_to_string(index: wellen::VarIndex) -> String {
    if index.msb() == index.lsb() {
        format!("[{}]", index.lsb())
    } else {
        format!("[{}:{}]", index.msb(), index.lsb())
    }
}

pub(crate) fn var_to_signal_type(signaltype: VarType) -> SignalType {
    match signaltype {
        VarType::Reg => SignalType::VCDReg,
        VarType::Wire => SignalType::VCDWire,
        VarType::Integer => SignalType::VCDInteger,
        VarType::Real => SignalType::VCDReal,
        VarType::Parameter => SignalType::VCDParameter,
        VarType::String => SignalType::VCDString,
        VarType::Time => SignalType::VCDTime,
        VarType::Event => SignalType::VCDEvent,
        VarType::Supply0 => SignalType::VCDSupply0,
        VarType::Supply1 => SignalType::VCDSupply1,
        VarType::Tri => SignalType::VCDTri,
        VarType::TriAnd => SignalType::VCDTriAnd,
        VarType::TriOr => SignalType::VCDTriOr,
        VarType::TriReg => SignalType::VCDTriReg,
        VarType::Tri0 => SignalType::VCDTri0,
        VarType::Tri1 => SignalType::VCDTri1,
        VarType::WAnd => SignalType::VCDWAnd,
        VarType::WOr => SignalType::VCDWOr,
        VarType::Port => SignalType::Port,
        VarType::Bit => SignalType::Bit,
        VarType::Logic => SignalType::Logic,
        VarType::Int => SignalType::VCDInteger,
        VarType::Enum => SignalType::Enum,
    }
}

#[inline]
fn binary_search(times: &[Time], needle: Time) -> usize {
    let mut lower_idx = 0usize;
    let mut upper_idx = times.len() - 1;
    while lower_idx <= upper_idx {
        let mid_idx = lower_idx + ((upper_idx - lower_idx) / 2);

        match times[mid_idx].cmp(&needle) {
            std::cmp::Ordering::Less => {
                lower_idx = mid_idx + 1;
            }
            std::cmp::Ordering::Equal => {
                return mid_idx;
            }
            std::cmp::Ordering::Greater => {
                upper_idx = mid_idx - 1;
            }
        }
    }
    lower_idx - 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signal_conversion() {
        let inp0: &[u8] = &[128, 0, 0, 3];
        let out0 = convert_signal_value(wellen::SignalValue::Binary(inp0, 32));
        assert_eq!(out0, SignalValue::BigUint(BigUint::from(0x80000003u64)));
    }
}
