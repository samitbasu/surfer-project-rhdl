use egui_remixicon::icons;
use surfer_translation_types::VariableDirection;

#[local_impl::local_impl]
impl VariableDirectionExt for VariableDirection {
    fn from_wellen_direction(direction: wellen::VarDirection) -> VariableDirection {
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

    fn get_icon(&self) -> Option<&str> {
        match self {
            VariableDirection::Unknown => None,
            VariableDirection::Implicit => None,
            VariableDirection::Input => Some(icons::CONTRACT_RIGHT_FILL),
            VariableDirection::Output => Some(icons::EXPAND_RIGHT_FILL),
            VariableDirection::InOut => Some(icons::ARROW_LEFT_RIGHT_LINE),
            VariableDirection::Buffer => None,
            VariableDirection::Linkage => Some(icons::LINK),
        }
    }
}
