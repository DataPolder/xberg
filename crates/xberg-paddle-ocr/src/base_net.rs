use crate::inference::{self, ModelBackend};
use crate::ocr_error::OcrError;

#[cfg(feature = "ort")]
use ort::session::builder::SessionBuilder;

pub trait BaseNet {
    fn new() -> Self;

    fn set_backend(&mut self, backend: Option<Box<dyn ModelBackend>>);

    /// Load a model from a file path onto the compile-time default backend (see
    /// [`inference::default_backend`]).
    fn init_model(&mut self, path: &str, num_thread: usize) -> Result<(), OcrError> {
        let model_bytes = std::fs::read(path)?;
        self.init_model_from_memory(&model_bytes, num_thread)
    }

    /// Load a model from ONNX bytes onto the compile-time default backend.
    fn init_model_from_memory(&mut self, model_bytes: &[u8], num_thread: usize) -> Result<(), OcrError> {
        // TODO(#1354): pin per Phase 0 spike outcome (currently no PaddleOCR model is pinned).
        let backend = inference::load_backend(inference::default_backend(), model_bytes, num_thread, None)?;
        self.set_backend(Some(backend));
        Ok(())
    }

    /// Load a model from a file path onto the `ort` backend with a custom session
    /// builder (e.g. for GPU execution providers).
    #[cfg(feature = "ort")]
    fn init_model_with_ort_builder(
        &mut self,
        path: &str,
        num_thread: usize,
        builder_fn: Option<fn(SessionBuilder) -> Result<SessionBuilder, ort::Error>>,
    ) -> Result<(), OcrError> {
        let model_bytes = std::fs::read(path)?;
        self.init_model_from_memory_with_ort_builder(&model_bytes, num_thread, builder_fn)
    }

    /// Load a model from ONNX bytes onto the `ort` backend with a custom session
    /// builder.
    #[cfg(feature = "ort")]
    fn init_model_from_memory_with_ort_builder(
        &mut self,
        model_bytes: &[u8],
        num_thread: usize,
        builder_fn: Option<fn(SessionBuilder) -> Result<SessionBuilder, ort::Error>>,
    ) -> Result<(), OcrError> {
        let backend = inference::ort_backend::OrtBackend::load(model_bytes, num_thread, builder_fn)?;
        self.set_backend(Some(Box::new(backend)));
        Ok(())
    }
}
