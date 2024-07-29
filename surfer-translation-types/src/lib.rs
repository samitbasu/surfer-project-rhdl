mod field_ref;
#[cfg(target_family = "unix")]
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
pub use crate::translator::{
    translates_all_bit_types, BasicTranslator, NumericTranslator, Translator,
};
pub use crate::variable_ref::VariableRef;

#[derive(Debug, PartialEq, Clone, Display)]
pub enum VariableValue {
    #[display(fmt = "{_0}")]
    BigUint(BigUint),
    #[display(fmt = "{_0}")]
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
    #[display(fmt = "event")]
    VCDEvent,
    #[display(fmt = "reg")]
    VCDReg,
    #[display(fmt = "wire")]
    VCDWire,
    #[display(fmt = "real")]
    VCDReal,
    #[display(fmt = "time")]
    VCDTime,
    #[display(fmt = "string")]
    VCDString,
    #[display(fmt = "parameter")]
    VCDParameter,
    #[display(fmt = "integer")]
    VCDInteger,
    #[display(fmt = "real time")]
    VCDRealTime,
    #[display(fmt = "supply 0")]
    VCDSupply0,
    #[display(fmt = "supply 1")]
    VCDSupply1,
    #[display(fmt = "tri")]
    VCDTri,
    #[display(fmt = "tri and")]
    VCDTriAnd,
    #[display(fmt = "tri or")]
    VCDTriOr,
    #[display(fmt = "tri reg")]
    VCDTriReg,
    #[display(fmt = "tri 0")]
    VCDTri0,
    #[display(fmt = "tri 1")]
    VCDTri1,
    #[display(fmt = "wand")]
    VCDWAnd,
    #[display(fmt = "wor")]
    VCDWOr,
    #[display(fmt = "port")]
    Port,
    #[display(fmt = "sparse array")]
    SparseArray,
    #[display(fmt = "realtime")]
    RealTime,

    // System Verilog
    #[display(fmt = "bit")]
    Bit,
    #[display(fmt = "logic")]
    Logic,
    #[display(fmt = "int")]
    Int,
    #[display(fmt = "shortint")]
    ShortInt,
    #[display(fmt = "longint")]
    LongInt,
    #[display(fmt = "byte")]
    Byte,
    #[display(fmt = "enum")]
    Enum,
    #[display(fmt = "shortreal")]
    ShortReal,

    // VHDL (these are the types emitted by GHDL)
    #[display(fmt = "boolean")]
    Boolean,
    #[display(fmt = "bit_vector")]
    BitVector,
    #[display(fmt = "std_logic")]
    StdLogic,
    #[display(fmt = "std_logic_vector")]
    StdLogicVector,
    #[display(fmt = "std_ulogic")]
    StdULogic,
    #[display(fmt = "std_ulogic_vector")]
    StdULogicVector,
}

#[derive(Clone, Display, Copy)]
pub enum VariableDirection {
    #[display(fmt = "unknown")]
    Unknown,
    #[display(fmt = "implicit")]
    Implicit,
    #[display(fmt = "input")]
    Input,
    #[display(fmt = "output")]
    Output,
    #[display(fmt = "inout")]
    InOut,
    #[display(fmt = "buffer")]
    Buffer,
    #[display(fmt = "linkage")]
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
