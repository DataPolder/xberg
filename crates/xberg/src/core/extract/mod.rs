//! Unified public extraction API.
//!
//! These functions are the stable, binding-generated public surface. Their
//! signatures must remain byte-identical (they are scanned by the alef binding
//! generator). The implementation delegates to a process-global default
//! [`crate::engine::Engine`]; the extraction internals live in
//! [`crate::engine`] and are a pure refactor of what previously lived here.

use std::sync::{Arc, LazyLock, Mutex};

use crate::Result;
#[cfg(feature = "url-ingestion")]
use crate::core::config::UrlExtractionConfig;
use crate::core::config::{ExtractInput, ExtractionConfig, ExtractionResult};
#[cfg(feature = "url-ingestion")]
use crawlberg::{CrawlEngine, MapResult};

/// Process-global default engine backing the free `extract` / `extract_batch`
/// functions. Construction is cheap and side-effect free.
static DEFAULT_ENGINE: LazyLock<crate::engine::Engine> = LazyLock::new(crate::engine::Engine::new_default);

/// Extract content from a single bytes or URI input.
pub async fn extract(input: ExtractInput, config: &ExtractionConfig) -> Result<ExtractionResult> {
    DEFAULT_ENGINE.extract(input, config).await
}

/// Extract content from multiple bytes or URI inputs.
pub async fn extract_batch(inputs: Vec<ExtractInput>, config: &ExtractionConfig) -> Result<ExtractionResult> {
    DEFAULT_ENGINE.extract_batch(inputs, config).await
}

/// Receives extraction progress without blocking the extraction worker.
pub trait ProgressListener: Send + Sync {
    /// Handle one progress event. Callback failures in language bindings are logged and ignored.
    fn on_progress(
        &self,
        stage: String,
        page: Option<usize>,
        total: Option<usize>,
        completed: Option<usize>,
        backend: Option<String>,
        input_index: Option<usize>,
    );
}

/// Shared listener handle used by language bindings.
#[cfg_attr(alef, alef(skip))]
pub type ProgressListenerHandle = Arc<Mutex<dyn ProgressListener + Send + Sync>>;

#[cfg(feature = "tokio-runtime")]
struct ListenerProgressSink {
    sender: std::sync::mpsc::Sender<ListenerProgressEvent>,
}

#[cfg(feature = "tokio-runtime")]
enum ListenerProgressEvent {
    OcrPage {
        page: usize,
        total: usize,
        completed: usize,
        backend: String,
        input_index: Option<usize>,
    },
}

#[cfg(feature = "tokio-runtime")]
impl crate::engine::seams::ProgressSink for ListenerProgressSink {
    fn emit(&self, _event: crate::engine::seams::ProgressEvent) {}

    fn emit_ocr_page(&self, page: usize, total: usize, completed: usize, backend: &str, input_index: Option<usize>) {
        let _ = self.sender.send(ListenerProgressEvent::OcrPage {
            page,
            total,
            completed,
            backend: backend.to_string(),
            input_index,
        });
    }
}

#[cfg(feature = "tokio-runtime")]
fn progress_forwarder(listener: ProgressListenerHandle) -> (Arc<ListenerProgressSink>, tokio::task::JoinHandle<()>) {
    let (sender, receiver) = std::sync::mpsc::channel::<ListenerProgressEvent>();
    let worker = tokio::task::spawn_blocking(move || {
        while let Ok(event) = receiver.recv() {
            let Ok(listener) = listener.lock() else {
                tracing::warn!("progress callback lock was poisoned; ignoring remaining events");
                return;
            };
            match event {
                ListenerProgressEvent::OcrPage {
                    page,
                    total,
                    completed,
                    backend,
                    input_index,
                } => listener.on_progress(
                    "ocr_page".to_string(),
                    Some(page),
                    Some(total),
                    Some(completed),
                    Some(backend),
                    input_index,
                ),
            }
        }
    });
    (Arc::new(ListenerProgressSink { sender }), worker)
}

#[cfg(feature = "tokio-runtime")]
async fn finish_progress_forwarder(worker: tokio::task::JoinHandle<()>) {
    if let Err(error) = worker.await {
        tracing::warn!(%error, "progress callback worker failed");
    }
}

/// Extract one input and report progress to `on_progress`.
#[cfg(feature = "tokio-runtime")]
#[cfg_attr(alef, alef(skip))]
pub async fn extract_with_progress(
    input: ExtractInput,
    config: &ExtractionConfig,
    on_progress: ProgressListenerHandle,
) -> Result<ExtractionResult> {
    let (sink, worker) = progress_forwarder(on_progress);
    let engine = crate::engine::Engine::builder()
        .with_progress_sink(sink.clone())
        .build();
    let result = engine.extract(input, config).await;
    drop(engine);
    drop(sink);
    finish_progress_forwarder(worker).await;
    result
}

/// Extract multiple inputs and report progress to `on_progress`.
#[cfg(feature = "tokio-runtime")]
#[cfg_attr(alef, alef(skip))]
pub async fn extract_batch_with_progress(
    inputs: Vec<ExtractInput>,
    config: &ExtractionConfig,
    on_progress: ProgressListenerHandle,
) -> Result<ExtractionResult> {
    let (sink, worker) = progress_forwarder(on_progress);
    let engine = crate::engine::Engine::builder()
        .with_progress_sink(sink.clone())
        .build();
    let result = engine.extract_batch(inputs, config).await;
    drop(engine);
    drop(sink);
    finish_progress_forwarder(worker).await;
    result
}

/// Discover all pages and sitemaps reachable from `uri` without extracting document content.
///
/// Builds a [`crawlberg::CrawlEngine`] from `config.crawl`, calls
/// [`CrawlEngine::map`], and returns the set of discovered URLs as a
/// [`crawlberg::MapResult`] (re-exported as [`crate::MapResult`]).
///
/// Use this when you need the URL inventory of a site before committing to
/// full document extraction — e.g. to build a crawl queue or validate scope.
///
/// # Errors
///
/// Returns [`crate::XbergError::Validation`] if the crawl configuration fails
/// validation or if the map operation itself fails.
#[cfg(feature = "url-ingestion")]
pub async fn map_url(uri: &str, config: &UrlExtractionConfig) -> Result<MapResult> {
    config.crawl.validate().map_err(map_crawl_err)?;
    let engine = CrawlEngine::builder()
        .config(config.crawl.clone())
        .build()
        .map_err(map_crawl_err)?;
    engine.map(uri).await.map_err(map_crawl_err)
}

/// Convert a [`crawlberg::CrawlError`] into an [`crate::XbergError`].
///
/// Mirrors the conversion used by the URL-ingestion extraction paths.
#[cfg(feature = "url-ingestion")]
fn map_crawl_err(error: crawlberg::CrawlError) -> crate::XbergError {
    crate::XbergError::validation(format!("crawlberg URL extraction failed: {error}"))
}

#[cfg(all(test, feature = "tokio-runtime"))]
mod tests {
    use super::*;
    use crate::engine::seams::ProgressSink;

    type RecordedEvent = (
        String,
        Option<usize>,
        Option<usize>,
        Option<usize>,
        Option<String>,
        Option<usize>,
        std::thread::ThreadId,
    );

    struct RecordingListener {
        events: Arc<Mutex<Vec<RecordedEvent>>>,
    }

    impl ProgressListener for RecordingListener {
        fn on_progress(
            &self,
            stage: String,
            page: Option<usize>,
            total: Option<usize>,
            completed: Option<usize>,
            backend: Option<String>,
            input_index: Option<usize>,
        ) {
            self.events.lock().expect("listener mutex poisoned").push((
                stage,
                page,
                total,
                completed,
                backend,
                input_index,
                std::thread::current().id(),
            ));
        }
    }

    #[tokio::test]
    async fn should_forward_ordered_progress_page_events_off_the_extraction_thread() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let listener: ProgressListenerHandle = Arc::new(Mutex::new(RecordingListener {
            events: Arc::clone(&events),
        }));
        let extraction_thread = std::thread::current().id();
        let (sink, worker) = progress_forwarder(listener);

        sink.emit_ocr_page(2, 4, 1, "tesseract", Some(7));
        sink.emit_ocr_page(4, 4, 2, "paddleocr", Some(7));
        drop(sink);
        finish_progress_forwarder(worker).await;

        let events = events.lock().expect("listener mutex poisoned");
        assert_eq!(events.len(), 2);
        assert_eq!(
            events[0],
            (
                "ocr_page".to_string(),
                Some(2),
                Some(4),
                Some(1),
                Some("tesseract".to_string()),
                Some(7),
                events[0].6,
            )
        );
        assert_eq!(events[1].1, Some(4));
        assert_eq!(events[1].3, Some(2));
        assert_ne!(events[0].6, extraction_thread);
        assert_eq!(events[0].6, events[1].6);
    }

    struct PanickingListener;

    impl ProgressListener for PanickingListener {
        fn on_progress(
            &self,
            _stage: String,
            _page: Option<usize>,
            _total: Option<usize>,
            _completed: Option<usize>,
            _backend: Option<String>,
            _input_index: Option<usize>,
        ) {
            panic!("callback failure");
        }
    }

    #[tokio::test]
    async fn should_ignore_progress_worker_failure() {
        let listener: ProgressListenerHandle = Arc::new(Mutex::new(PanickingListener));
        let (sink, worker) = progress_forwarder(listener);

        sink.emit_ocr_page(1, 1, 1, "tesseract", None);
        drop(sink);
        finish_progress_forwarder(worker).await;
    }
}
