use std::collections::HashMap;

use camino::Utf8PathBuf;
use color_eyre::{
    eyre::{bail, Context},
    Result,
};
use fastwave_backend::{Signal, SignalValue};
use pyo3::{
    intern, pyclass, pymethods, pymodule,
    types::{IntoPyDict, PyModule, PyString},
    PyObject, PyResult, Python, ToPyObject,
};

use crate::benchmark::TimedRegion;

use super::{SignalInfo, TranslationResult, Translator, ValueRepr, ValueColor};

pub struct PyTranslator {
    name: String,
    instance: PyObject,
}

impl PyTranslator {
    pub fn new(
        name: &str,
        source: impl Into<Utf8PathBuf>,
        args: HashMap<String, String>,
    ) -> Result<Self> {
        let source = source.into();
        let code =
            std::fs::read_to_string(&source).with_context(|| format!("Failed to read {source}"))?;

        let instance = Python::with_gil(|py| -> Result<PyObject> {
            let module = PyModule::from_code(py, &code, source.as_str(), &name)
                .with_context(|| format!("Failed to load {name} in {source}"))?;

            let class = module.getattr(name)?;

            let instance = class.call((), Some(args.iter().into_py_dict(py)))?;

            let ensure_has_attr = |attr: &str| {
                if !instance.hasattr(attr)? {
                    bail!("Translator {name} does not have a `{attr}` method");
                }
                Ok(())
            };

            ensure_has_attr("translates")?;
            ensure_has_attr("translate")?;
            ensure_has_attr("signal_info")?;

            Ok(instance.to_object(py))
        })?;

        Ok(Self {
            name: name.to_string(),
            instance,
        })
    }
}

impl Translator for PyTranslator {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn translate(&self, signal: &Signal, value: &SignalValue) -> Result<TranslationResult> {
        let mut stringify = TimedRegion::defer();
        let mut gil_lock = TimedRegion::defer();
        let mut name_push = TimedRegion::defer();
        let mut value_push = TimedRegion::defer();
        let mut method_call = TimedRegion::defer();
        let mut extraction = TimedRegion::defer();
        let mut gil_unlock = TimedRegion::defer();

        stringify.start();
        let value_str = match value {
            SignalValue::BigUint(val) => format!(
                "{val:0width$b}",
                width = signal.num_bits().unwrap_or(0) as usize
            ),
            SignalValue::String(val) => val.clone(),
        };
        stringify.stop();

        gil_lock.start();
        let mut result = Python::with_gil(|py| -> Result<TranslationResult> {
            gil_lock.stop();

            method_call.start();

            name_push.start();
            let name_py = PyString::new(py, &signal.name());
            name_push.stop();
            value_push.start();
            let val_py = PyString::new(py, &value_str);
            value_push.stop();

            let result = self
                .instance
                .call_method1(py, intern!(py, "translate"), (name_py, val_py))
                .with_context(|| format!("Failed to run translates on {}", self.name))?;
            method_call.stop();

            extraction.start();
            let val: PyTranslationResult = result.extract(py)?;
            extraction.stop();

            gil_unlock.start();
            Ok(val.0)
        })?;
        gil_unlock.stop();

        result.push_duration("stringify", stringify.secs());
        result.push_duration("gil_lock", gil_lock.secs());
        result.push_duration("name_push", name_push.secs());
        result.push_duration("value_push", value_push.secs());
        result.push_duration("extraction", extraction.secs());
        result.push_duration("gil_unlock", gil_unlock.secs());
        result.push_duration(
            "method_call_overhead",
            method_call.secs()
                - result.durations["spade_pythonify"]
                - result.durations["spade_prelude"]
                - result.durations["spade_translate"],
        );

        Ok(result)
    }

    fn signal_info(&self, _signal: &Signal, name: &str) -> Result<SignalInfo> {
        Python::with_gil(|py| {
            let result = self
                .instance
                .call_method1(py, intern!(py, "signal_info"), (name,))
                .with_context(|| format!("Error when running signal_info on {}", self.name))?;

            let val: Option<PySignalInfo> = result.extract(py)?;
            Ok(val.map(|s| s.0).unwrap_or(SignalInfo::Bits))
        })
    }

    fn translates(&self, signal: &Signal) -> Result<bool> {
        let name = signal.name();

        Python::with_gil(|py| {
            let result = self
                .instance
                .call_method1(py, intern!(py, "translates"), (name,))
                .with_context(|| format!("Error when running translates on {}", self.name))?;

            let val: bool = result.extract(py)?;
            Ok(val)
        })
    }
}

#[pyclass(name = "TranslationResult")]
#[derive(Clone)]
struct PyTranslationResult(TranslationResult);

#[pymethods]
impl PyTranslationResult {
    #[new]
    fn new() -> Self {
        Self(TranslationResult {
            // TODO: Set this
            val: ValueRepr::Bits(0, "".to_string()),
            subfields: vec![],
            durations: HashMap::new(),
            color: ValueColor::Normal
        })
    }

    fn repr_string(&mut self, val: &str) {
         self.0.val = ValueRepr::String(val.to_string());
    }
    fn repr_bits(&mut self) {
        // TODO: Set this
         self.0.val = ValueRepr::Bits(0, "".to_string())
    }
    fn repr_tuple(&mut self) {
         self.0.val = ValueRepr::Tuple
    }
    fn repr_struct(&mut self) {
         self.0.val = ValueRepr::Struct
    }
    fn repr_array(&mut self) {
         self.0.val = ValueRepr::Array
    }

    fn with_fields(&mut self, fields: Vec<(String, PyTranslationResult)>) {
        self.0.subfields = fields.into_iter().map(|(n, v)| (n, v.0)).collect()
    }

    pub fn push_duration(&mut self, name: &str, duration: f64) {
        self.0.push_duration(name, duration);
    }
}

#[pyclass(name = "SignalInfo")]
#[derive(Clone)]
struct PySignalInfo(SignalInfo);

#[pymethods]
impl PySignalInfo {
    #[new]
    fn new() -> Self {
        Self(SignalInfo::Bits)
    }

    pub fn with_field(&mut self, field: (String, PySignalInfo)) {
        let unpacked = (field.0, field.1 .0);
        match &mut self.0 {
            SignalInfo::Bits | SignalInfo::Bool => {
                self.0 = SignalInfo::Compound {
                    subfields: vec![unpacked],
                }
            }
            SignalInfo::Compound { subfields } => subfields.push(unpacked),
        }
    }
}

/// The python stuff we expose to python plugins. This must be apended to
/// the import stuff before python code is run, preferably by
/// ```
/// append_to_inittab!(surfer);
/// ```
/// early on in the program
#[pymodule]
pub fn surfer(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<PyTranslationResult>()?;
    m.add_class::<PySignalInfo>()?;
    Ok(())
}
