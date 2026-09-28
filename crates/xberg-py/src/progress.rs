use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;

struct PyProgressListener {
    callback: Py<PyAny>,
}

impl xberg::ProgressListener for PyProgressListener {
    fn on_progress(
        &self,
        stage: String,
        page: Option<usize>,
        total: Option<usize>,
        completed: Option<usize>,
        backend: Option<String>,
        input_index: Option<usize>,
    ) {
        Python::attach(|py| {
            if let Err(error) =
                self.callback
                    .call_method1(py, "on_progress", (stage, page, total, completed, backend, input_index))
            {
                tracing::warn!(%error, "progress callback raised; ignoring");
            }
        });
    }
}

fn listener_handle(on_progress: Py<PyAny>) -> PyResult<xberg::ProgressListenerHandle> {
    Python::attach(|py| {
        if !on_progress.bind(py).hasattr("on_progress")? {
            return Err(pyo3::exceptions::PyAttributeError::new_err(
                "progress listener is missing on_progress",
            ));
        }
        Ok(
            std::sync::Arc::new(std::sync::Mutex::new(PyProgressListener { callback: on_progress }))
                as xberg::ProgressListenerHandle,
        )
    })
}

#[pyfunction]
#[pyo3(signature = (input, config, on_progress))]
pub fn extract_with_progress<'py>(
    py: Python<'py>,
    input: crate::ExtractInput,
    config: crate::ExtractionConfig,
    on_progress: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let listener = listener_handle(on_progress)?;
    let input = input.into();
    let config = config.into();
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        xberg::extract_with_progress(input, &config, listener)
            .await
            .map(crate::ExtractionResult::from)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))
    })
}

#[pyfunction]
#[pyo3(signature = (inputs, config, on_progress))]
pub fn extract_batch_with_progress<'py>(
    py: Python<'py>,
    inputs: Vec<crate::ExtractInput>,
    config: crate::ExtractionConfig,
    on_progress: Py<PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let listener = listener_handle(on_progress)?;
    let inputs = inputs.into_iter().map(Into::into).collect();
    let config = config.into();
    pyo3_async_runtimes::tokio::future_into_py(py, async move {
        xberg::extract_batch_with_progress(inputs, &config, listener)
            .await
            .map(crate::ExtractionResult::from)
            .map_err(|error| PyRuntimeError::new_err(error.to_string()))
    })
}

#[cfg(test)]
mod tests {
    use pyo3::ffi::c_str;
    use pyo3::types::PyModule;

    use super::*;

    #[test]
    fn should_deliver_exact_progress_values_to_python() {
        Python::initialize();
        Python::attach(|py| {
            let module = PyModule::from_code(
                py,
                c_str!(
                    r#"
events = []
class Listener:
    def on_progress(self, *args):
        events.append(args)
listener = Listener()
"#
                ),
                c_str!("progress_test.py"),
                c_str!("progress_test"),
            )
            .expect("test Python module should compile");
            let listener = listener_handle(module.getattr("listener").expect("listener should exist").unbind())
                .expect("listener bridge should be created");
            let bridge = listener.lock().expect("listener mutex poisoned");

            xberg::ProgressListener::on_progress(
                &*bridge,
                "ocr_page".to_string(),
                Some(3),
                Some(9),
                Some(4),
                Some("tesseract".to_string()),
                Some(2),
            );

            let event: (String, usize, usize, usize, String, usize) = module
                .getattr("events")
                .expect("events should exist")
                .get_item(0)
                .expect("event should be recorded")
                .extract()
                .expect("event should contain typed values");
            assert_eq!(event, ("ocr_page".to_string(), 3, 9, 4, "tesseract".to_string(), 2));
        });
    }

    #[test]
    fn should_ignore_python_callback_exception() {
        Python::initialize();
        Python::attach(|py| {
            let module = PyModule::from_code(
                py,
                c_str!(
                    r#"
class Listener:
    def on_progress(self, *args):
        raise RuntimeError("callback failed")
listener = Listener()
"#
                ),
                c_str!("progress_error_test.py"),
                c_str!("progress_error_test"),
            )
            .expect("test Python module should compile");
            let listener = listener_handle(module.getattr("listener").expect("listener should exist").unbind())
                .expect("listener bridge should be created");
            let bridge = listener.lock().expect("listener mutex poisoned");

            xberg::ProgressListener::on_progress(
                &*bridge,
                "ocr_page".to_string(),
                Some(1),
                Some(1),
                Some(1),
                Some("tesseract".to_string()),
                None,
            );
        });
    }
}
