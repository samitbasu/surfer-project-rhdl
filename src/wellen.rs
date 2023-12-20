use crate::time::{TimeScale, TimeUnit};
use crate::variable_type::VariableType;
use color_eyre::eyre::bail;
use color_eyre::{eyre::anyhow, Result};
use log::warn;
use num::{BigUint, ToPrimitive};
use std::fmt::Write;
use wellen::{
    self, GetItem, ScopeType, Time, TimeTableIdx, Timescale, TimescaleUnit, Var, VarRef, VarType,
    Waveform,
};

use crate::wave_container::{
    MetaData, QueryResult, ScopeRef, VariableMeta, VariableRef, VariableValue,
};

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

    pub fn variable_names(&self) -> Vec<String> {
        self.vars.clone()
    }

    fn lookup_scope(&self, scope: &ScopeRef) -> Option<wellen::ScopeRef> {
        match scope.get_wellen_id() {
            Some(id) => Some(id),
            None => self.inner.hierarchy().lookup_scope(scope.strs()),
        }
    }
    pub fn variables(&self) -> Vec<VariableRef> {
        let h = self.inner.hierarchy();
        h.iter_vars()
            .map(|r| VariableRef::from_hierarchy_string(&r.full_name(h)))
            .collect::<Vec<_>>()
    }

    pub fn variables_in_scope(&self, scope_ref: &ScopeRef) -> Vec<VariableRef> {
        let h = self.inner.hierarchy();
        // special case of an empty scope means that we want to variables that are part of the toplevel
        if scope_ref.strs().is_empty() {
            h.vars()
                .map(|id| {
                    VariableRef::new_with_wave_id(
                        scope_ref.clone(),
                        h.get(id).name(h).to_string(),
                        id,
                    )
                })
                .collect::<Vec<_>>()
        } else {
            let scope = match self.lookup_scope(scope_ref) {
                Some(id) => h.get(id),
                None => {
                    warn!("Found no scope '{scope_ref}'. Defaulting to no variables");
                    return vec![];
                }
            };
            scope
                .vars(h)
                .map(|id| {
                    VariableRef::new_with_wave_id(
                        scope_ref.clone(),
                        h.get(id).name(h).to_string(),
                        id,
                    )
                })
                .collect::<Vec<_>>()
        }
    }

    pub fn update_variable_ref(&self, variable: &VariableRef) -> Option<VariableRef> {
        // IMPORTANT: lookup by name!
        let h = self.inner.hierarchy();

        // first we lookup the scope in order to update the scope reference
        let scope = h.lookup_scope(variable.path.strs())?;
        let new_scope_ref = variable.path.with_wellen_id(scope);

        // now we lookup the variable
        let var = h
            .get(scope)
            .vars(h)
            .find(|r| h.get(*r).name(h) == variable.name)?;
        let new_variable_ref =
            VariableRef::new_with_wave_id(new_scope_ref, variable.name.clone(), var);
        Some(new_variable_ref)
    }

    pub fn get_var(&self, r: &VariableRef) -> Result<&Var> {
        let h = self.inner.hierarchy();
        self.get_var_ref(r).map(|r| h.get(r))
    }

    fn get_var_ref(&self, r: &VariableRef) -> Result<VarRef> {
        match r.get_wellen_id() {
            Some(id) => Ok(id),
            None => {
                let h = self.inner.hierarchy();
                let var = match h.lookup_var(r.path.strs(), &r.name) {
                    None => bail!("Failed to find variable: {r:?}"),
                    Some(id) => id,
                };
                Ok(var)
            }
        }
    }

    pub fn load_variable(&mut self, r: &VariableRef) -> Result<()> {
        let var_ref = self.get_var_ref(r)?;
        let signal_ref = self.inner.hierarchy().get(var_ref).signal_ref();
        self.inner.load_signals(&[signal_ref]);
        Ok(())
    }

    pub fn load_variables<S: AsRef<VariableRef>, T: Iterator<Item = S>>(
        &mut self,
        variables: T,
    ) -> Result<()> {
        let h = self.inner.hierarchy();
        let signal_refs = variables
            .flat_map(|s| {
                let r = s.as_ref();
                self.get_var_ref(r).map(|v| h.get(v).signal_ref())
            })
            .collect::<Vec<_>>();
        self.inner.load_signals(&signal_refs);
        Ok(())
    }

    fn time_to_time_table_idx(&self, time: &BigUint) -> Option<TimeTableIdx> {
        let time: Time = time.to_u64().expect("unsupported time!");
        let table = self.inner.time_table();
        if table.is_empty() || table[0] > time {
            None
        } else {
            // binary search to find correct index
            let idx = binary_search(table, time);
            assert!(table[idx] <= time);
            Some(idx as TimeTableIdx)
        }
    }

    pub fn query_variable(&self, variable: &VariableRef, time: &BigUint) -> Result<QueryResult> {
        let h = self.inner.hierarchy();
        // find variable from string
        let var_ref = self.get_var_ref(variable)?;
        // map variable to variable ref
        let signal_ref = h.get(var_ref).signal_ref();
        let sig = match self.inner.get_signal(signal_ref) {
            Some(sig) => sig,
            None => bail!("internal error: variable {variable:?} should have been loaded!"),
        };
        let time_table = self.inner.time_table();

        // convert time to index
        if let Some(idx) = self.time_to_time_table_idx(time) {
            // get data offset
            if let Some(offset) = sig.get_offset(idx) {
                // which time did we actually get the value for?
                let offset_time_idx = sig.get_time_idx_at(&offset);
                let offset_time = time_table[offset_time_idx as usize];
                // get the last value in a time step (since we ignore delta cycles for now)
                let current_value = sig.get_value_at(&offset, offset.elements - 1);
                // the next time the variable changes
                let next_time = offset
                    .next_index
                    .and_then(|i| time_table.get(i.get() as usize));

                let converted_value = convert_variable_value(current_value);
                let result = QueryResult {
                    current: Some((BigUint::from(offset_time), converted_value)),
                    next: next_time.map(|t| BigUint::from(*t)),
                };
                return Ok(result);
            }
        }

        // if `get_offset` returns None, this means that there is no change at or before the requested time
        let first_index = sig.get_first_time_idx();
        let next_time = first_index.and_then(|i| time_table.get(i as usize));
        let result = QueryResult {
            current: None,
            next: next_time.map(|t| BigUint::from(*t)),
        };
        Ok(result)
    }

    pub fn scope_names(&self) -> Vec<String> {
        self.scopes.clone()
    }

    pub fn root_scopes(&self) -> Vec<ScopeRef> {
        let h = self.inner.hierarchy();
        h.scopes()
            .map(|id| ScopeRef::from_strs_with_wellen_id(&[h.get(id).name(h)], id))
            .collect::<Vec<_>>()
    }

    pub fn child_scopes(&self, scope_ref: &ScopeRef) -> Result<Vec<ScopeRef>> {
        let h = self.inner.hierarchy();
        let scope = match self.lookup_scope(scope_ref) {
            Some(id) => h.get(id),
            None => return Err(anyhow!("Failed to find scope {scope_ref:?}")),
        };
        Ok(scope
            .scopes(h)
            .map(|id| scope_ref.with_subscope(h.get(id).name(h).to_string()))
            .collect::<Vec<_>>())
    }

    pub fn scope_exists(&self, scope: &ScopeRef) -> bool {
        self.lookup_scope(scope).is_some()
    }

    pub fn get_scope_tooltip_data(&self, scope: &ScopeRef) -> String {
        let mut out = String::new();
        if let Some(scope_ref) = self.lookup_scope(scope) {
            let h = self.inner.hierarchy();
            let scope = h.get(scope_ref);
            writeln!(&mut out, "{}", scope_type_to_string(scope.scope_type())).unwrap();
            if let Some((path, line)) = scope.instantiation_source_loc(h) {
                writeln!(&mut out, "{path}:{line}").unwrap();
            }
            match (scope.component(h), scope.source_loc(h)) {
                (Some(name), Some((path, line))) => {
                    write!(&mut out, "{name} : {path}:{line}").unwrap()
                }
                (None, Some((path, line))) => {
                    // check to see if instance and definition are the same
                    let same = scope
                        .instantiation_source_loc(h)
                        .map(|(i_path, i_line)| path == i_path && line == i_line)
                        .unwrap_or(false);
                    if !same {
                        write!(&mut out, "{path}:{line}").unwrap()
                    }
                }
                (Some(name), None) => write!(&mut out, "{name}").unwrap(),
                // remove possible trailing new line
                (None, None) => {}
            }
        }
        if out.ends_with('\n') {
            out.pop().unwrap();
        }
        out
    }
}

fn scope_type_to_string(tpe: ScopeType) -> &'static str {
    match tpe {
        ScopeType::Module => "module",
        ScopeType::Task => "task",
        ScopeType::Function => "function",
        ScopeType::Begin => "begin",
        ScopeType::Fork => "fork",
        ScopeType::Generate => "generate",
        ScopeType::Struct => "struct",
        ScopeType::Union => "union",
        ScopeType::Class => "class",
        ScopeType::Interface => "interface",
        ScopeType::Package => "package",
        ScopeType::Program => "program",
        ScopeType::VhdlArchitecture => "architecture",
        ScopeType::VhdlProcedure => "procedure",
        ScopeType::VhdlFunction => "function",
        ScopeType::VhdlRecord => "record",
        ScopeType::VhdlProcess => "process",
        ScopeType::VhdlBlock => "block",
        ScopeType::VhdlForGenerate => "for-generate",
        ScopeType::VhdlIfGenerate => "if-generate",
        ScopeType::VhdlGenerate => "generate",
        ScopeType::VhdlPackage => "package",
        ScopeType::GhwGeneric => "generic",
        ScopeType::VhdlArray => "array",
    }
}

fn convert_variable_value(value: wellen::SignalValue) -> VariableValue {
    match value {
        wellen::SignalValue::Binary(data, _bits) => {
            VariableValue::BigUint(BigUint::from_bytes_be(data))
        }
        wellen::SignalValue::FourValue(_, _) | wellen::SignalValue::NineValue(_, _) => {
            VariableValue::String(
                value
                    .to_bit_string()
                    .expect("failed to convert value {value:?} to a string"),
            )
        }
        wellen::SignalValue::String(value) => VariableValue::String(value.to_string()),
        wellen::SignalValue::Real(value) => VariableValue::String(format!("{}", value)),
    }
}

pub(crate) fn var_to_meta<'a>(var: &Var, r: &VariableRef) -> VariableMeta {
    VariableMeta {
        var: r.clone(),
        num_bits: var.length(),
        variable_type: Some(var.var_type().into()),
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

impl From<VarType> for VariableType {
    fn from(signaltype: VarType) -> Self {
        match signaltype {
            VarType::Reg => VariableType::VCDReg,
            VarType::Wire => VariableType::VCDWire,
            VarType::Integer => VariableType::VCDInteger,
            VarType::Real => VariableType::VCDReal,
            VarType::Parameter => VariableType::VCDParameter,
            VarType::String => VariableType::VCDString,
            VarType::Time => VariableType::VCDTime,
            VarType::Event => VariableType::VCDEvent,
            VarType::Supply0 => VariableType::VCDSupply0,
            VarType::Supply1 => VariableType::VCDSupply1,
            VarType::Tri => VariableType::VCDTri,
            VarType::TriAnd => VariableType::VCDTriAnd,
            VarType::TriOr => VariableType::VCDTriOr,
            VarType::TriReg => VariableType::VCDTriReg,
            VarType::Tri0 => VariableType::VCDTri0,
            VarType::Tri1 => VariableType::VCDTri1,
            VarType::WAnd => VariableType::VCDWAnd,
            VarType::WOr => VariableType::VCDWOr,
            VarType::Port => VariableType::Port,
            VarType::Bit => VariableType::Bit,
            VarType::Logic => VariableType::Logic,
            VarType::Int => VariableType::VCDInteger,
            VarType::Enum => VariableType::Enum,
            VarType::SparseArray => VariableType::SparseArray,
            VarType::RealTime => VariableType::RealTime,
            VarType::ShortInt => VariableType::ShortInt,
            VarType::LongInt => VariableType::LongInt,
            VarType::Byte => VariableType::Byte,
            VarType::ShortReal => VariableType::ShortReal,
            VarType::Boolean => VariableType::Boolean,
            VarType::BitVector => VariableType::BitVector,
            VarType::StdLogic => VariableType::StdLogic,
            VarType::StdLogicVector => VariableType::StdLogicVector,
            VarType::StdULogic => VariableType::StdULogic,
            VarType::StdULogicVector => VariableType::StdULogicVector,
        }
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
        let out0 = convert_variable_value(wellen::SignalValue::Binary(inp0, 32));
        assert_eq!(out0, VariableValue::BigUint(BigUint::from(0x80000003u64)));
    }
}
