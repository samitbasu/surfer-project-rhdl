use std::collections::{HashMap, HashSet};
use std::fmt::Write;

use color_eyre::{eyre::anyhow, eyre::bail, Result};
use derive_more::Debug;
use log::warn;
use num::{BigUint, ToPrimitive};
use surfer_translation_types::{VariableDirection, VariableEncoding, VariableType, VariableValue};
use wellen::{
    FileFormat, GetItem as _, Hierarchy, ScopeType, Signal, SignalEncoding, SignalRef,
    SignalSource, Time, TimeTable, TimeTableIdx, Timescale, TimescaleUnit, Var, VarRef, VarType,
};

use crate::message::BodyResult;
use crate::time::{TimeScale, TimeUnit};
use crate::variable_direction::VariableDirectionExt;
use crate::variable_type::VariableTypeExt;
use crate::wave_container::{
    MetaData, QueryResult, ScopeId, ScopeRef, ScopeRefExt, VarId, VariableMeta, VariableRef,
    VariableRefExt,
};

static UNIQUE_ID_COUNT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(Debug)]
pub struct WellenContainer {
    #[debug(skip)]
    hierarchy: std::sync::Arc<Hierarchy>,
    /// the url of a remote server, None if waveforms are loaded locally
    server: Option<String>,
    scopes: Vec<String>,
    vars: Vec<String>,
    signals: HashMap<SignalRef, Signal>,
    /// keeps track of signals that need to be loaded once the body of the waveform file has been loaded
    signals_to_be_loaded: HashSet<SignalRef>,
    time_table: TimeTable,
    #[debug(skip)]
    source: Option<SignalSource>,
    unique_id: u64,
    body_loaded: bool,
}

/// Returned by `load_variables` if we want to load the variables on a background thread.
/// This struct is currently only used by wellen
pub struct LoadSignalsCmd {
    signals: Vec<SignalRef>,
    from_unique_id: u64,
    payload: LoadSignalPayload,
}

pub enum LoadSignalPayload {
    Local(SignalSource, std::sync::Arc<Hierarchy>),
    Remote(String),
}

impl LoadSignalsCmd {
    pub fn destruct(self) -> (Vec<SignalRef>, u64, LoadSignalPayload) {
        (self.signals, self.from_unique_id, self.payload)
    }
}

pub struct LoadSignalsResult {
    source: Option<SignalSource>,
    server: Option<String>,
    signals: Vec<(SignalRef, Signal)>,
    from_unique_id: u64,
}

impl LoadSignalsResult {
    pub fn local(
        source: SignalSource,
        signals: Vec<(SignalRef, Signal)>,
        from_unique_id: u64,
    ) -> Self {
        Self {
            source: Some(source),
            server: None,
            signals,
            from_unique_id,
        }
    }

    pub fn remote(server: String, signals: Vec<(SignalRef, Signal)>, from_unique_id: u64) -> Self {
        Self {
            source: None,
            server: Some(server),
            signals,
            from_unique_id,
        }
    }

    pub fn len(&self) -> usize {
        self.signals.len()
    }
}

pub fn convert_format(format: FileFormat) -> crate::WaveFormat {
    match format {
        FileFormat::Vcd => crate::WaveFormat::Vcd,
        FileFormat::Fst => crate::WaveFormat::Fst,
        FileFormat::Ghw => crate::WaveFormat::Ghw,
        FileFormat::Unknown => unreachable!("should never get here"),
    }
}

impl WellenContainer {
    pub fn new(hierarchy: std::sync::Arc<Hierarchy>, server: Option<String>) -> Self {
        // generate a list of names for all variables and scopes since they will be requested by the parser
        let h = &hierarchy;
        let scopes = h
            .iter_scopes()
            .map(|r| r.full_name(h).to_string())
            .collect::<Vec<_>>();
        let vars = h
            .iter_vars()
            .map(|r| r.full_name(h).to_string())
            .collect::<Vec<_>>();

        let unique_id = UNIQUE_ID_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        Self {
            hierarchy,
            server,
            scopes,
            vars,
            signals: HashMap::new(),
            signals_to_be_loaded: HashSet::new(),
            time_table: vec![],
            source: None,
            unique_id,
            body_loaded: false,
        }
    }

    pub fn body_loaded(&self) -> bool {
        self.body_loaded
    }

    pub fn add_body(&mut self, body: BodyResult) -> Result<Option<LoadSignalsCmd>> {
        if self.body_loaded {
            bail!("Did we just parse the body twice? That should not happen!");
        }
        match body {
            BodyResult::Local(body) => {
                if self.server.is_some() {
                    bail!("We are connected to a server, but also received the result of parsing a file locally. Something is going wrong here!");
                }
                self.time_table = body.time_table;
                self.source = Some(body.source);
            }
            BodyResult::Remote(time_table, server) => {
                if let Some(old) = &self.server {
                    if old != &server {
                        bail!("Inconsistent server URLs: {old} vs. {server}")
                    }
                } else {
                    bail!("Missing server URL!");
                }
                self.time_table = time_table;
            }
        }
        self.body_loaded = true;

        // we might have to load some signals that the user has already added while the
        // body of the waveform file was being parser
        Ok(self.load_signals(&[]))
    }

    pub fn metadata(&self) -> MetaData {
        let timescale = self
            .hierarchy
            .timescale()
            .unwrap_or(Timescale::new(1, TimescaleUnit::Unknown));
        let date = None;
        MetaData {
            date,
            version: Some(self.hierarchy.version().to_string()),
            timescale: TimeScale {
                unit: TimeUnit::from(timescale.unit),
                multiplier: Some(timescale.factor),
            },
        }
    }

    pub fn max_timestamp(&self) -> Option<BigUint> {
        self.time_table.last().map(|t| BigUint::from(*t))
    }

    pub fn is_fully_loaded(&self) -> bool {
        (self.source.is_some() || self.server.is_some()) && self.signals_to_be_loaded.is_empty()
    }

    pub fn variable_names(&self) -> Vec<String> {
        self.vars.clone()
    }

    fn lookup_scope(&self, scope: &ScopeRef) -> Option<wellen::ScopeRef> {
        match scope.id {
            ScopeId::Wellen(id) => Some(id),
            ScopeId::None => self.hierarchy.lookup_scope(scope.strs()),
        }
    }
    pub fn variables(&self) -> Vec<VariableRef> {
        let h = &self.hierarchy;
        h.iter_vars()
            .map(|r| VariableRef::from_hierarchy_string(&r.full_name(h)))
            .collect::<Vec<_>>()
    }

    pub fn variables_in_scope(&self, scope_ref: &ScopeRef) -> Vec<VariableRef> {
        let h = &self.hierarchy;
        // special case of an empty scope means that we want to variables that are part of the toplevel
        if scope_ref.strs().is_empty() {
            h.vars()
                .map(|id| {
                    VariableRef::new_with_id(
                        scope_ref.clone(),
                        h.get(id).name(h).to_string(),
                        VarId::Wellen(id),
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
                    VariableRef::new_with_id(
                        scope_ref.clone(),
                        h.get(id).name(h).to_string(),
                        VarId::Wellen(id),
                    )
                })
                .collect::<Vec<_>>()
        }
    }

    pub fn no_variables_in_scope(&self, scope_ref: &ScopeRef) -> bool {
        let h = &self.hierarchy;
        // special case of an empty scope means that we want to variables that are part of the toplevel
        if scope_ref.strs().is_empty() {
            h.vars().collect::<Vec<_>>().is_empty()
        } else {
            let scope = match self.lookup_scope(scope_ref) {
                Some(id) => h.get(id),
                None => {
                    warn!("Found no scope '{scope_ref}'. Defaulting to no variables");
                    return true;
                }
            };
            scope.vars(h).collect::<Vec<_>>().is_empty()
        }
    }

    pub fn update_variable_ref(&self, variable: &VariableRef) -> Option<VariableRef> {
        // IMPORTANT: lookup by name!
        let h = &self.hierarchy;

        // first we lookup the scope in order to update the scope reference
        let scope = h.lookup_scope(variable.path.strs())?;
        let new_scope_ref = variable.path.with_id(ScopeId::Wellen(scope));

        // now we lookup the variable
        let var = h
            .get(scope)
            .vars(h)
            .find(|r| h.get(*r).name(h) == variable.name)?;
        let new_variable_ref =
            VariableRef::new_with_id(new_scope_ref, variable.name.clone(), VarId::Wellen(var));
        Some(new_variable_ref)
    }

    pub fn get_var(&self, r: &VariableRef) -> Result<&Var> {
        let h = &self.hierarchy;
        self.get_var_ref(r).map(|r| h.get(r))
    }

    pub fn get_enum_map(&self, v: &Var) -> HashMap<String, String> {
        match v.enum_type(&self.hierarchy) {
            None => HashMap::new(),
            Some((_, mapping)) => HashMap::from_iter(
                mapping
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v.to_string())),
            ),
        }
    }

    fn get_var_ref(&self, r: &VariableRef) -> Result<VarRef> {
        match r.id {
            VarId::Wellen(id) => Ok(id),
            VarId::None => {
                let h = &self.hierarchy;
                let var = match h.lookup_var(r.path.strs(), &r.name) {
                    None => bail!("Failed to find variable: {r:?}"),
                    Some(id) => id,
                };
                Ok(var)
            }
        }
    }

    pub fn load_variables<S: AsRef<VariableRef>, T: Iterator<Item = S>>(
        &mut self,
        variables: T,
    ) -> Result<Option<LoadSignalsCmd>> {
        let h = &self.hierarchy;
        let signal_refs = variables
            .flat_map(|s| {
                let r = s.as_ref();
                self.get_var_ref(r).map(|v| h.get(v).signal_ref())
            })
            .collect::<Vec<_>>();
        Ok(self.load_signals(&signal_refs))
    }

    pub fn on_signals_loaded(&mut self, res: LoadSignalsResult) -> Result<Option<LoadSignalsCmd>> {
        // check to see if this command came from our container, or from a previous file that was open
        if res.from_unique_id == self.unique_id {
            // return source or server
            debug_assert!(self.source.is_none());
            debug_assert!(self.server.is_none());
            self.source = res.source;
            self.server = res.server;
            debug_assert!(self.server.is_some() || self.source.is_some());
            // install signals
            for (id, signal) in res.signals {
                self.signals.insert(id, signal);
            }
        }

        // see if there are any more signals to dispatch
        Ok(self.load_signals(&[]))
    }

    fn load_signals(&mut self, ids: &[SignalRef]) -> Option<LoadSignalsCmd> {
        // make sure that we do not load signals that have already been loaded
        let filtered_ids = ids
            .iter()
            .filter(|id| !self.signals.contains_key(id) && !self.signals_to_be_loaded.contains(id))
            .cloned()
            .collect::<Vec<_>>();

        // add signals to signals that need to be loaded
        self.signals_to_be_loaded.extend(filtered_ids.iter());

        if self.signals_to_be_loaded.is_empty() {
            return None; // nothing to do here
        }

        if !self.body_loaded {
            return None; // it only makes sense to load signals after we have loaded the body
        }

        // we remove the server name in order to ensure that we do not load the same signal twice
        if let Some(server) = std::mem::take(&mut self.server) {
            // load remote signals
            let mut signals = Vec::from_iter(self.signals_to_be_loaded.drain());
            signals.sort(); // for some determinism!
            let cmd = LoadSignalsCmd {
                signals,
                payload: LoadSignalPayload::Remote(server),
                from_unique_id: self.unique_id,
            };
            Some(cmd)
        } else if let Some(source) = std::mem::take(&mut self.source) {
            // if we have a source available, let's load all signals!
            let mut signals = Vec::from_iter(self.signals_to_be_loaded.drain());
            signals.sort(); // for some determinism!
            let cmd = LoadSignalsCmd {
                signals,
                payload: LoadSignalPayload::Local(source, self.hierarchy.clone()),
                from_unique_id: self.unique_id,
            };
            Some(cmd)
        } else {
            None
        }
    }

    fn time_to_time_table_idx(&self, time: &BigUint) -> Option<TimeTableIdx> {
        let time: Time = time.to_u64().expect("unsupported time!");
        let table = &self.time_table;
        if table.is_empty() || table[0] > time {
            None
        } else {
            // binary search to find correct index
            let idx = binary_search(table, time);
            assert!(table[idx] <= time);
            Some(idx as TimeTableIdx)
        }
    }

    pub fn query_variable(
        &self,
        variable: &VariableRef,
        time: &BigUint,
    ) -> Result<Option<QueryResult>> {
        let h = &self.hierarchy;
        // find variable from string
        let var_ref = self.get_var_ref(variable)?;
        // map variable to variable ref
        let signal_ref = h.get(var_ref).signal_ref();
        let sig = match self.signals.get(&signal_ref) {
            Some(sig) => sig,
            None => {
                // if the signal has not been loaded yet, we return an empty result
                return Ok(None);
            }
        };
        let time_table = &self.time_table;

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
                return Ok(Some(result));
            }
        }

        // if `get_offset` returns None, this means that there is no change at or before the requested time
        let first_index = sig.get_first_time_idx();
        let next_time = first_index.and_then(|i| time_table.get(i as usize));
        let result = QueryResult {
            current: None,
            next: next_time.map(|t| BigUint::from(*t)),
        };
        Ok(Some(result))
    }

    pub fn scope_names(&self) -> Vec<String> {
        self.scopes.clone()
    }

    pub fn root_scopes(&self) -> Vec<ScopeRef> {
        let h = &self.hierarchy;
        h.scopes()
            .map(|id| ScopeRef::from_strs_with_id(&[h.get(id).name(h)], ScopeId::Wellen(id)))
            .collect::<Vec<_>>()
    }

    pub fn child_scopes(&self, scope_ref: &ScopeRef) -> Result<Vec<ScopeRef>> {
        let h = &self.hierarchy;
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
            let h = &self.hierarchy;
            let scope = h.get(scope_ref);
            writeln!(&mut out, "{}", scope_type_to_string(scope.scope_type())).unwrap();
            if let Some((path, line)) = scope.instantiation_source_loc(h) {
                writeln!(&mut out, "{path}:{line}").unwrap();
            }
            match (scope.component(h), scope.source_loc(h)) {
                (Some(name), Some((path, line))) => {
                    write!(&mut out, "{name} : {path}:{line}").unwrap();
                }
                (None, Some((path, line))) => {
                    // check to see if instance and definition are the same
                    let same = scope
                        .instantiation_source_loc(h)
                        .is_some_and(|(i_path, i_line)| path == i_path && line == i_line);
                    if !same {
                        write!(&mut out, "{path}:{line}").unwrap();
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
        wellen::SignalValue::Real(value) => VariableValue::String(format!("{value}")),
    }
}

pub(crate) fn var_to_meta(
    var: &Var,
    enum_map: HashMap<String, String>,
    r: &VariableRef,
) -> VariableMeta {
    let encoding = match var.signal_encoding() {
        SignalEncoding::String => VariableEncoding::String,
        SignalEncoding::Real => VariableEncoding::Real,
        SignalEncoding::BitVector(_) => VariableEncoding::BitVector,
    };
    VariableMeta {
        var: r.clone(),
        num_bits: var.length(),
        variable_type: Some(VariableType::from_wellen_type(var.var_type())),
        index: var.index().map(index_to_string),
        direction: Some(VariableDirection::from_wellen_direction(var.direction())),
        enum_map,
        encoding,
    }
}

fn index_to_string(index: wellen::VarIndex) -> String {
    if index.msb() == index.lsb() {
        format!("[{}]", index.lsb())
    } else {
        format!("[{}:{}]", index.msb(), index.lsb())
    }
}

#[local_impl::local_impl]
impl FromVarType for VariableType {
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
