mod field_ref;
#[cfg(feature = "pyo3")]
pub mod python;
mod result;
mod scope_ref;
mod translator;
mod variable_ref;

use std::collections::HashMap;

use derive_more::Display;
use ecolor::Color32;
use num::BigUint;

pub use crate::field_ref::FieldRef;
pub use crate::result::{
    HierFormatResult, SubFieldFlatTranslationResult, SubFieldTranslationResult, TranslatedValue,
    TranslationResult, ValueRepr,
};
pub use crate::scope_ref::ScopeRef;
pub use crate::translator::{translates_all_bit_types, BasicTranslator, Translator};
pub use crate::variable_ref::VariableRef;

#[derive(Debug, PartialEq, Clone, Display)]
pub enum VariableValue {
    #[display("{_0}")]
    BigUint(BigUint),
    #[display("{_0}")]
    String(String),
}

#[derive(Clone, PartialEq, Copy, Debug)]
pub enum ValueKind {
    Normal,
    Undef,
    HighImp,
    Custom(Color32),
    Warn,
    DontCare,
    Weak,
}

#[derive(PartialEq)]
pub enum TranslationPreference {
    /// This translator prefers translating the variable, so it will be selected
    /// as the default translator for the variable
    Prefer,
    /// This translator is able to translate the variable, but will not be
    /// selected by default, the user has to select it
    Yes,
    No,
}

/// Static information about the structure of a variable.
#[derive(Clone, Debug, Default)]
pub enum VariableInfo {
    Compound {
        subfields: Vec<(String, VariableInfo)>,
    },
    Bits,
    Bool,
    Clock,
    // NOTE: only used for state saving where translators will clear this out with the actual value
    #[default]
    String,
    Real,
}

#[derive(Debug, Display, Clone, Copy, Eq, PartialEq)]
pub enum VariableType {
    // VCD-specific types
    #[display("event")]
    VCDEvent,
    #[display("reg")]
    VCDReg,
    #[display("wire")]
    VCDWire,
    #[display("real")]
    VCDReal,
    #[display("time")]
    VCDTime,
    #[display("string")]
    VCDString,
    #[display("parameter")]
    VCDParameter,
    #[display("integer")]
    VCDInteger,
    #[display("real time")]
    VCDRealTime,
    #[display("supply 0")]
    VCDSupply0,
    #[display("supply 1")]
    VCDSupply1,
    #[display("tri")]
    VCDTri,
    #[display("tri and")]
    VCDTriAnd,
    #[display("tri or")]
    VCDTriOr,
    #[display("tri reg")]
    VCDTriReg,
    #[display("tri 0")]
    VCDTri0,
    #[display("tri 1")]
    VCDTri1,
    #[display("wand")]
    VCDWAnd,
    #[display("wor")]
    VCDWOr,
    #[display("port")]
    Port,
    #[display("sparse array")]
    SparseArray,
    #[display("realtime")]
    RealTime,

    // System Verilog
    #[display("bit")]
    Bit,
    #[display("logic")]
    Logic,
    #[display("int")]
    Int,
    #[display("shortint")]
    ShortInt,
    #[display("longint")]
    LongInt,
    #[display("byte")]
    Byte,
    #[display("enum")]
    Enum,
    #[display("shortreal")]
    ShortReal,

    // VHDL (these are the types emitted by GHDL)
    #[display("boolean")]
    Boolean,
    #[display("bit_vector")]
    BitVector,
    #[display("std_logic")]
    StdLogic,
    #[display("std_logic_vector")]
    StdLogicVector,
    #[display("std_ulogic")]
    StdULogic,
    #[display("std_ulogic_vector")]
    StdULogicVector,
}

#[derive(Clone, Display, Copy)]
pub enum VariableDirection {
    #[display("unknown")]
    Unknown,
    #[display("implicit")]
    Implicit,
    #[display("input")]
    Input,
    #[display("output")]
    Output,
    #[display("inout")]
    InOut,
    #[display("buffer")]
    Buffer,
    #[display("linkage")]
    Linkage,
}

#[derive(Clone)]
pub struct VariableMeta<VarId, ScopeId> {
    pub var: VariableRef<VarId, ScopeId>,
    pub num_bits: Option<u32>,
    /// Type of the variable in the HDL (on a best effort basis).
    pub variable_type: Option<VariableType>,
    pub index: Option<String>,
    pub direction: Option<VariableDirection>,
    pub enum_map: HashMap<String, String>,
    /// Indicates how the variable is stored. A variable of "type" boolean for example
    /// could be stored as a String or as a BitVector.
    pub encoding: VariableEncoding,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum VariableEncoding {
    String,
    Real,
    BitVector,
}
