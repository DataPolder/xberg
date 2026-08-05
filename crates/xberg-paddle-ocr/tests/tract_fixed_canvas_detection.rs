#![cfg(feature = "tract")] // ~keep: exercises the tract-only fixed-canvas detection path
//! End-to-end check that DBNet loads and detects under `tract` when pinned to a fixed canvas.
//!
//! `#[ignore]`d because it needs a real PaddleOCR detection model on disk. It reads the model
//! from the local Hugging Face cache only — it never downloads — and skips with a message when
//! the cache is not populated. Run it with:
//!
//! ```text
//! cargo test -p xberg-paddle-ocr --no-default-features --features tract \
//!     --test tract_fixed_canvas_detection -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use xberg_paddle_ocr::base_net::BaseNet;
use xberg_paddle_ocr::db_net::DbNet;
use xberg_paddle_ocr::scale_param::ScaleParam;

const MODEL_REPO_SNAPSHOT: &str =
    ".cache/huggingface/hub/models--xberg-io--paddleocr-onnx-models/snapshots/bfaf0b492cfc1dee0c73245fc5860bfdcf2c3443";
const DETECTION_MODEL: &str = "v2/det/mobile.onnx";
const DETECTION_CANVAS: u32 = 640;

fn cached_detection_model() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let path = PathBuf::from(home).join(MODEL_REPO_SNAPSHOT).join(DETECTION_MODEL);
    path.exists().then_some(path)
}

/// A white page with two solid black bars — high-contrast regions DBNet reliably fires on.
fn page_with_text_like_bars() -> image::RgbImage {
    let mut page = image::RgbImage::from_pixel(400, 200, image::Rgb([255, 255, 255]));
    for y in 40..70 {
        for x in 30..370 {
            page.put_pixel(x, y, image::Rgb([0, 0, 0]));
        }
    }
    for y in 120..150 {
        for x in 30..200 {
            page.put_pixel(x, y, image::Rgb([0, 0, 0]));
        }
    }
    page
}

#[test]
#[ignore = "requires the PaddleOCR detection model in the local Hugging Face cache"]
fn should_detect_text_boxes_under_tract_with_a_pinned_detection_canvas() {
    let Some(model_path) = cached_detection_model() else {
        panic!("model not in the local Hugging Face cache: ~/{MODEL_REPO_SNAPSHOT}/{DETECTION_MODEL}");
    };

    let mut db_net = DbNet::new();
    db_net
        .init_model_with_canvas(
            model_path.to_str().expect("model path is valid UTF-8"),
            1,
            Some(DETECTION_CANVAS),
        )
        .expect("DBNet must load under tract when pinned to a fixed square canvas");

    let page = page_with_text_like_bars();
    let scale = ScaleParam::get_scale_param(&page, DETECTION_CANVAS);
    assert!(
        scale.dst_width <= DETECTION_CANVAS && scale.dst_height <= DETECTION_CANVAS,
        "resized page {}x{} must fit the canvas",
        scale.dst_width,
        scale.dst_height
    );

    let text_boxes = db_net
        .get_text_boxes(&page, &scale, 0.5, 0.3, 1.6)
        .expect("pinned tract detection must run at the canvas it was built for");

    assert!(
        !text_boxes.is_empty(),
        "two solid black bars on a white page must produce at least one detection"
    );
    for text_box in &text_boxes {
        for point in &text_box.points {
            assert!(
                point.x <= page.width() && point.y <= page.height(),
                "detection {point:?} escaped the page — padded-region output leaked past the crop"
            );
        }
    }
}
