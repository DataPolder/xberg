//! Shared normalization constants for PaddleOCR preprocessing.
//!
//! Two normalization schemes are used:
//!
//! - **ImageNet** (`IMAGENET_MEAN_VALUES` / `IMAGENET_NORM_VALUES`): used by the text
//!   detection network (`DbNet`) and the angle classifier (`AngleNet`).
//!   Formula: `(pixel - mean * 255) * (1 / (std * 255))`.
//!
//! - **CRNN** (`CRNN_MEAN_VALUES` / `CRNN_NORM_VALUES`): used by the text recognition
//!   network (`CrnnNet`).
//!   Formula: `(pixel - 127.5) * (1 / 127.5)`.

/// ImageNet channel means (R, G, B), pre-multiplied by 255.
///
/// Derived from `[0.485, 0.456, 0.406]` (per-channel ImageNet means).
/// Used by `DbNet` (text detection) and `AngleNet` (angle classification).
pub(crate) const IMAGENET_MEAN_VALUES: [f32; 3] = [0.485 * 255.0, 0.456 * 255.0, 0.406 * 255.0];

/// ImageNet channel normalization factors (R, G, B), equal to `1 / (std * 255)`.
///
/// Derived from `[0.229, 0.224, 0.225]` (per-channel ImageNet standard deviations).
/// Used by `DbNet` (text detection) and `AngleNet` (angle classification).
pub(crate) const IMAGENET_NORM_VALUES: [f32; 3] = [1.0 / (0.229 * 255.0), 1.0 / (0.224 * 255.0), 1.0 / (0.225 * 255.0)];

/// CRNN channel means (R, G, B): `127.5` for all channels.
///
/// Used by `CrnnNet` (text recognition).
pub(crate) const CRNN_MEAN_VALUES: [f32; 3] = [127.5, 127.5, 127.5];

/// CRNN channel normalization factors (R, G, B): `1 / 127.5` for all channels.
///
/// Used by `CrnnNet` (text recognition).
pub(crate) const CRNN_NORM_VALUES: [f32; 3] = [1.0 / 127.5, 1.0 / 127.5, 1.0 / 127.5];

/// Source pixel value that fills the right/bottom padding when detection input is blitted
/// into a fixed square canvas (the `tract` detection path -- see `DbNet::detection_canvas`).
///
/// White, run through the *same* `pixel * norm - mean * norm` formula as real pixels, so the
/// padding lands on exactly the value `DbNet`'s normalization already produces for blank page
/// background. Two reasons this is the right blank:
///
/// - DBNet is trained on document/scene pages whose background is white, so a white region is
///   what the network has learned to score as "no text".
/// - This crate already pads detection input with white
///   (`OcrUtils::make_padding`, applied by `PaddleOcrEngine::detect_base_detailed` before
///   scaling), so canvas padding is indistinguishable from padding DBNet already sees today.
///
/// Filling with the post-normalization zero point instead (raw ~124/116/104 mid-gray) would
/// draw a high-contrast straight edge down the content boundary of a white page, which DBNet
/// can score as a stroke and turn into a spurious box.
pub(crate) const DETECTION_CANVAS_PAD_PIXEL: f32 = 255.0;
