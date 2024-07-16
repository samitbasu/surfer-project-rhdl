use ecolor::Color32;

use crate::ValueKind;

#[pyo3::pymodule]
#[pyo3(name = "surfer")]
pub fn surfer_pyo3_module(m: &pyo3::Bound<'_, pyo3::types::PyModule>) -> pyo3::PyResult<()> {
    m.add_class::<PythonValueKind>().unwrap();
    Ok(())
}

#[derive(Clone)]
#[pyo3::pyclass(name = "ValueKind")]
pub enum PythonValueKind {
    Normal {},
    Undef {},
    HighImp {},
    Custom { color: [u8; 4] },
    Warn {},
    DontCare {},
    Weak {},
}

impl From<PythonValueKind> for ValueKind {
    fn from(value: PythonValueKind) -> Self {
        match value {
            PythonValueKind::Normal {} => ValueKind::Normal,
            PythonValueKind::Undef {} => ValueKind::Undef,
            PythonValueKind::HighImp {} => ValueKind::HighImp,
            PythonValueKind::Custom {
                color: [r, g, b, a],
            } => ValueKind::Custom(Color32::from_rgba_unmultiplied(r, g, b, a)),
            PythonValueKind::Warn {} => ValueKind::Undef,
            PythonValueKind::DontCare {} => ValueKind::Undef,
            PythonValueKind::Weak {} => ValueKind::Undef,
        }
    }
}
