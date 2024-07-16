use color_eyre::Result;
use log::error;
use pyo3::types::{PyAnyMethods, PyDict, PyModule, PyStringMethods};
use pyo3::{Bound, Py, Python};
use surfer_translation_types::python::{surfer_pyo3_module, PythonValueKind};
use surfer_translation_types::{BasicTranslator, ValueKind, VariableValue};

use crate::wave_container::{ScopeId, VarId};

pub struct PythonTranslator {
    plugin: Py<PyModule>,
}

impl PythonTranslator {
    pub fn new(code: &str) -> Result<Self> {
        let plugin = Python::with_gil(|py| -> pyo3::PyResult<_> {
            let surfer_module = PyModule::new_bound(py, "surfer")?;
            surfer_pyo3_module(&surfer_module)?;
            let sys = PyModule::import_bound(py, "sys")?;
            let py_modules: Bound<'_, PyDict> = sys.getattr("modules")?.downcast_into()?;
            py_modules.set_item("surfer", surfer_module)?;
            let module = PyModule::from_code_bound(py, code, "plugin.py", "plugin")?;
            Ok(module.unbind())
        })?;
        Ok(Self { plugin })
    }
}

impl BasicTranslator<VarId, ScopeId> for PythonTranslator {
    fn name(&self) -> String {
        Python::with_gil(|py| {
            self.plugin
                .bind(py)
                .getattr("name")
                .unwrap()
                .call0()
                .unwrap()
                .str()
                .unwrap()
                .to_string_lossy()
                .to_string()
        })
    }

    fn basic_translate(&self, num_bits: u64, value: &VariableValue) -> (String, ValueKind) {
        let result = Python::with_gil(|py| -> pyo3::PyResult<_> {
            let ret = self
                .plugin
                .bind(py)
                .getattr("basic_translate")?
                .call((num_bits, value.to_string()), None)?;
            let ret = ret.downcast()?;
            let v = ret.get_item(0).unwrap().extract().unwrap();
            let k = ValueKind::from(
                ret.get_item(1)?
                    .downcast::<PythonValueKind>()?
                    .get()
                    .clone(),
            );
            Ok((v, k))
        });
        match result {
            Ok((v, k)) => (v, k),
            Err(e) => {
                error!(
                    "Could not translate '{}' with Python translator '{}': {}",
                    value,
                    self.name(),
                    e
                );
                (value.to_string(), ValueKind::Normal)
            }
        }
    }
}
