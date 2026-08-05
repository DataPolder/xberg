#![cfg(feature = "tract")] // ~keep: exercises the tract-only fixed-canvas detection path
//! End-to-end check that DBNet loads and detects under `tract` when pinned to a fixed canvas.
//!
//! `#[ignore]`d because it needs a real PaddleOCR detection model on disk. It reads the model
//! from the local Hugging Face cache only — it never downloads — and fails with a clear message
//! when the cache is not populated. Run it with:
//!
//! ```text
//! cargo test --release -p xberg-paddle-ocr --no-default-features --features tract \
//!     --test tract_fixed_canvas_detection -- --ignored --nocapture
//! ```

use std::path::PathBuf;

use xberg_paddle_ocr::base_net::BaseNet;
use xberg_paddle_ocr::db_net::DbNet;
use xberg_paddle_ocr::scale_param::ScaleParam;

// Must track `HF_REPO_REVISION` in `xberg`'s paddle_ocr::model_manager; this crate cannot
// reference that constant, so the revision is repeated here. ~keep
const MODEL_REPO_SNAPSHOT: &str =
    ".cache/huggingface/hub/models--xberg-io--paddleocr-onnx-models/snapshots/bc5ec866cf0e798e667808dfa51b0ba8ad0dafc8";
const DETECTION_MODEL: &str = "v2/det/mobile.onnx";
const DETECTION_CANVAS: u32 = 640;
const PAGE_WIDTH: u32 = 640;
const PAGE_HEIGHT: u32 = 320;

fn cached_detection_model() -> PathBuf {
    let home = std::env::var_os("HOME").expect("HOME is set");
    let path = PathBuf::from(home).join(MODEL_REPO_SNAPSHOT).join(DETECTION_MODEL);
    assert!(
        path.exists(),
        "model not in the local Hugging Face cache: {}",
        path.display()
    );
    path
}

/// Draw a row of blocky `E` glyphs — thin strokes at a text-like scale and stroke density,
/// which DBNet responds to far more reliably than one solid bar.
fn draw_text_row(page: &mut image::RgbImage, top: u32, glyph_count: u32) {
    const GLYPH_WIDTH: u32 = 14;
    const GLYPH_HEIGHT: u32 = 22;
    const STROKE: u32 = 3;
    const ADVANCE: u32 = 20;
    let black = image::Rgb([0, 0, 0]);

    for glyph in 0..glyph_count {
        let left = 30 + glyph * ADVANCE;
        for y in top..top + GLYPH_HEIGHT {
            for x in left..left + STROKE {
                page.put_pixel(x, y, black);
            }
        }
        for bar in 0..3 {
            let bar_top = top + bar * (GLYPH_HEIGHT - STROKE) / 2;
            for y in bar_top..bar_top + STROKE {
                for x in left..left + GLYPH_WIDTH {
                    page.put_pixel(x, y, black);
                }
            }
        }
    }
}

/// A white page carrying two rows of text-like glyphs.
fn page_with_text_rows(width: u32, height: u32) -> image::RgbImage {
    let mut page = image::RgbImage::from_pixel(width, height, image::Rgb([255, 255, 255]));
    draw_text_row(&mut page, 60, 24);
    draw_text_row(&mut page, 160, 16);
    page
}

fn load_pinned_detector(canvas: u32) -> DbNet {
    let model_path = cached_detection_model();
    let mut db_net = DbNet::new();
    db_net
        .init_model_with_canvas(model_path.to_str().expect("model path is valid UTF-8"), 1, Some(canvas))
        .expect("DBNet must load under tract when pinned to a fixed square canvas");
    db_net
}

#[test]
#[ignore = "requires the PaddleOCR detection model in the local Hugging Face cache"]
fn should_detect_text_boxes_under_tract_with_a_pinned_detection_canvas() {
    let db_net = load_pinned_detector(DETECTION_CANVAS);

    let page = page_with_text_rows(PAGE_WIDTH, PAGE_HEIGHT);
    let scale = ScaleParam::get_scale_param(&page, DETECTION_CANVAS);
    assert_eq!(
        (scale.dst_width, scale.dst_height),
        (PAGE_WIDTH, PAGE_HEIGHT),
        "the page must land inside the canvas with padding below it"
    );

    let text_boxes = db_net
        .get_text_boxes(&page, &scale, 0.5, 0.3, 1.6)
        .expect("pinned tract detection must run at the canvas it was built for");

    println!("detections on the padded canvas: {}", text_boxes.len());
    assert!(
        !text_boxes.is_empty(),
        "two rows of text-like glyphs must produce at least one detection"
    );
    for text_box in &text_boxes {
        for point in &text_box.points {
            assert!(
                point.x <= PAGE_WIDTH && point.y <= PAGE_HEIGHT,
                "detection {point:?} escaped the page — padded-region output leaked past the crop"
            );
        }
    }
}

/// The core correctness claim of the fixed-canvas design: filling the canvas remainder with the
/// normalized value of white is equivalent to having padded the page with real white pixels
/// before normalization. Run against a page that is already canvas-sized (so the padded path
/// degenerates to no padding at all) versus the same content padded in tensor space.
#[test]
#[ignore = "requires the PaddleOCR detection model in the local Hugging Face cache"]
fn should_match_detection_on_a_page_pre_padded_with_real_white_pixels() {
    let db_net = load_pinned_detector(DETECTION_CANVAS);

    let page = page_with_text_rows(PAGE_WIDTH, PAGE_HEIGHT);
    let padded_in_tensor_space = db_net
        .get_text_boxes(
            &page,
            &ScaleParam::get_scale_param(&page, DETECTION_CANVAS),
            0.5,
            0.3,
            1.6,
        )
        .expect("detection on the padded canvas must run");

    // Same content, but the canvas remainder is real white pixels in the source image, so
    // `ScaleParam` already produces a square page and no tensor padding is added.
    let pre_padded = page_with_text_rows(DETECTION_CANVAS, DETECTION_CANVAS);
    let pre_padded_scale = ScaleParam::get_scale_param(&pre_padded, DETECTION_CANVAS);
    assert_eq!(
        (pre_padded_scale.dst_width, pre_padded_scale.dst_height),
        (DETECTION_CANVAS, DETECTION_CANVAS),
        "the control page must exactly fill the canvas"
    );
    let padded_in_image_space = db_net
        .get_text_boxes(&pre_padded, &pre_padded_scale, 0.5, 0.3, 1.6)
        .expect("detection on the pre-padded page must run");

    println!(
        "tensor-space padding: {} detections, image-space padding: {} detections",
        padded_in_tensor_space.len(),
        padded_in_image_space.len()
    );
    assert!(
        !padded_in_tensor_space.is_empty(),
        "the comparison is only meaningful with detections present"
    );
    assert_eq!(
        padded_in_tensor_space.len(),
        padded_in_image_space.len(),
        "tensor-space padding must detect the same regions as real white pixels"
    );
    for (tensor_box, image_box) in padded_in_tensor_space.iter().zip(&padded_in_image_space) {
        assert_eq!(
            tensor_box.points, image_box.points,
            "tensor-space padding must place boxes identically to real white pixels"
        );
    }
}
