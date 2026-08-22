//! Thread-safety tests: concurrent reads and renders of a shared document.
//!
//! This replaces a test that previously built its fixture through
//! `crate::ffi::pdf_document_builder_*`, the C FFI builder API. That module
//! (`src/ffi.rs`) has been removed from this fork, so the fixtures here are
//! built directly with `xberg_native_pdf::api::Pdf`. The subject under test is
//! unchanged: `PdfDocument`'s internal `Mutex`-guarded reader
//! (`lock_or_recover`, see `src/document.rs`) must make concurrent access to
//! one shared handle safe, whether that handle is reached through the C ABI
//! or, as here, directly through `Arc<PdfDocument>` on the Rust API.
//!
//! * `concurrent_document_reads_no_panic` — 8 threads each open their own
//!   handle from shared bytes and extract text.
//! * `concurrent_renders_no_panic` — 8 threads render the same page from
//!   independent handles opened from shared bytes.
//! * `concurrent_render_page_fit_one_shared_handle_no_spurious_parse` — many
//!   threads call `render_page_fit` on a single *shared* `PdfDocument`,
//!   exercising the same internal reader-lock serialization that the C ABI
//!   depends on.
//! * `concurrent_render_embedded_font_no_spurious_parse` — same shared-handle
//!   pattern, but against a PDF with an embedded font (markdown-rendered),
//!   which exercises the embedded-font cmap classifier under concurrency.

use std::sync::Arc;
use xberg_native_pdf::api::Pdf;
use xberg_native_pdf::document::PdfDocument;

#[test]
fn concurrent_document_reads_no_panic() {
    let pdf_bytes: Arc<Vec<u8>> = Arc::new(Pdf::from_text("Concurrent read test").expect("build PDF").into_bytes());

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let bytes = Arc::clone(&pdf_bytes);
            std::thread::spawn(move || {
                let doc = PdfDocument::from_bytes((*bytes).clone()).expect("open failed in thread");
                let text = doc.extract_text(0).expect("extract_text failed in thread");
                assert!(text.contains("Concurrent"), "unexpected text content: {text:.100}");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread panicked");
    }
}

/// Rendering pipeline (tiny-skia, font rasteriser, etc.) must be safe to
/// call from multiple threads at once when each thread has its own handle
/// opened from shared bytes.
#[test]
fn concurrent_renders_no_panic() {
    use xberg_native_pdf::rendering::RenderOptions;

    let bytes: Arc<Vec<u8>> = Arc::new(
        Pdf::from_text("Concurrent render test")
            .expect("build PDF")
            .into_bytes(),
    );
    let opts = Arc::new(RenderOptions::with_dpi(72));

    let handles: Vec<_> = (0..8)
        .map(|_| {
            let b = Arc::clone(&bytes);
            let o = Arc::clone(&opts);
            std::thread::spawn(move || {
                let doc = PdfDocument::from_bytes((*b).clone()).expect("open PDF in thread");
                let img = xberg_native_pdf::rendering::render_page(&doc, 0, &o).expect("render must not fail");
                assert!(!img.data.is_empty(), "rendered image data must not be empty");
                assert!(img.width > 0 && img.height > 0, "rendered dimensions must be positive");
            })
        })
        .collect();

    for h in handles {
        h.join().expect("render thread panicked");
    }
}

/// Many threads calling `render_page_fit` against ONE shared `PdfDocument`
/// must never surface a spurious parse error. This is the direct Rust
/// equivalent of hammering a single shared FFI document handle from
/// multiple threads — `Arc<PdfDocument>` here plays the role the C binding's
/// shared native pointer played, and both routes rely on the same internal
/// `lock_or_recover()` serialization.
#[test]
fn concurrent_render_page_fit_one_shared_handle_no_spurious_parse() {
    use xberg_native_pdf::rendering::{RenderOptions, render_page_fit};

    const THREADS: usize = 8;
    const ITERS: usize = 16;

    let bytes = Pdf::from_text("Shared-handle render race regression")
        .expect("build PDF")
        .into_bytes();
    let doc = Arc::new(PdfDocument::from_bytes(bytes).expect("open_from_bytes failed"));
    let opts = Arc::new(RenderOptions::default());

    let barrier = Arc::new(std::sync::Barrier::new(THREADS));
    let handles: Vec<_> = (0..THREADS)
        .map(|_| {
            let doc = Arc::clone(&doc);
            let opts = Arc::clone(&opts);
            let b = Arc::clone(&barrier);
            std::thread::spawn(move || -> Result<(), String> {
                b.wait();
                for i in 0..ITERS {
                    match render_page_fit(&doc, 0, 200, 200, &opts) {
                        Ok(img) => {
                            if img.data.is_empty() {
                                return Err(format!("iter {i}: render produced empty data"));
                            }
                        }
                        Err(e) => {
                            return Err(format!("iter {i}: render failed: {e}"));
                        }
                    }
                }
                Ok(())
            })
        })
        .collect();

    let mut failures = Vec::new();
    for h in handles {
        match h.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => failures.push(e),
            Err(_) => failures.push("render thread panicked".to_string()),
        }
    }

    assert!(failures.is_empty(), "shared-handle render race: {failures:?}");
}

/// Same shared-handle pattern as above, but against a PDF with an embedded
/// font (produced via markdown rendering), which exercises the embedded-font
/// cmap classifier under concurrency rather than the built-in Helvetica path.
#[test]
fn concurrent_render_embedded_font_no_spurious_parse() {
    use xberg_native_pdf::rendering::{RenderOptions, render_page, render_page_fit};

    const THREADS: usize = 8;
    const ITERS: usize = 16;

    let bytes = Pdf::from_markdown("# Thread Safety\n\nPage 1.\n\n---\n\nPage 2.\n\n---\n\nPage 3.")
        .expect("build markdown PDF")
        .into_bytes();
    let doc = Arc::new(PdfDocument::from_bytes(bytes).expect("open_from_bytes failed"));
    let opts = Arc::new(RenderOptions::default());
    let pages = doc.page_count().expect("page_count failed");
    assert!(pages >= 1, "expected at least one page, got {pages}");

    let barrier = Arc::new(std::sync::Barrier::new(THREADS));
    let handles: Vec<_> = (0..THREADS)
        .map(|t| {
            let doc = Arc::clone(&doc);
            let opts = Arc::clone(&opts);
            let b = Arc::clone(&barrier);
            std::thread::spawn(move || -> Result<(), String> {
                b.wait();
                for i in 0..ITERS {
                    let page = i % pages;
                    let result = if (t + i) % 2 == 0 {
                        render_page_fit(&doc, page, 200, 260, &opts)
                    } else {
                        render_page(&doc, page, &opts)
                    };
                    match result {
                        Ok(img) if !img.data.is_empty() => {}
                        Ok(_) => {
                            return Err(format!("thread {t} iter {i} page {page}: empty render"));
                        }
                        Err(e) => return Err(format!("thread {t} iter {i} page {page}: {e}")),
                    }
                }
                Ok(())
            })
        })
        .collect();

    let mut failures = Vec::new();
    for h in handles {
        match h.join() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => failures.push(e),
            Err(_) => failures.push("render thread panicked".to_string()),
        }
    }

    assert!(
        failures.is_empty(),
        "embedded-font shared-handle render race: {failures:?}"
    );
}
