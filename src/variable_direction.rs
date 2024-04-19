use core::fmt;

use egui_remixicon::icons;

#[derive(Clone)]
pub enum VariableDirection {
    Unknown,
    Implicit,
    Input,
    Output,
    InOut,
    Buffer,
    Linkage,
}

impl From<wellen::VarDirection> for VariableDirection {
    fn from(direction: wellen::VarDirection) -> Self {
        match direction {
            wellen::VarDirection::Unknown => VariableDirection::Unknown,
            wellen::VarDirection::Implicit => VariableDirection::Implicit,
            wellen::VarDirection::Input => VariableDirection::Input,
            wellen::VarDirection::Output => VariableDirection::Output,
            wellen::VarDirection::InOut => VariableDirection::InOut,
            wellen::VarDirection::Buffer => VariableDirection::Buffer,
            wellen::VarDirection::Linkage => VariableDirection::Linkage,
        }
    }
}

impl VariableDirection {
    pub fn get_icon(&self) -> &str {
        match self {
            VariableDirection::Unknown => "    ",
            VariableDirection::Implicit => "    ",
            VariableDirection::Input => icons::CONTRACT_RIGHT_FILL,
            VariableDirection::Output => icons::EXPAND_RIGHT_FILL,
            VariableDirection::InOut => icons::ARROW_LEFT_RIGHT_LINE,
            VariableDirection::Buffer => "    ",
            VariableDirection::Linkage => icons::LINK,
        }
    }
}

impl fmt::Display for VariableDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VariableDirection::Unknown => write!(f, "unknown"),
            VariableDirection::Implicit => write!(f, "implicit"),
            VariableDirection::Input => write!(f, "input"),
            VariableDirection::Output => write!(f, "output"),
            VariableDirection::InOut => write!(f, "inout"),
            VariableDirection::Buffer => write!(f, "buffer"),
            VariableDirection::Linkage => write!(f, "linkage"),
        }
    }
}
