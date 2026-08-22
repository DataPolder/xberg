//! Appearance stream generation for PDF annotations.
//!
//! This module provides support for generating appearance streams (AP dictionaries)
//! for annotations, ensuring consistent visual rendering across PDF viewers.
//!
//! PDF Spec: ISO 32000-1:2008, Section 12.5.5 (Appearance Streams)
//!
//! # Appearance Stream Structure
//!
//! An appearance stream is a Form XObject that defines the visual appearance
//! of an annotation. The AP dictionary can contain:
//! - /N - Normal appearance (required for most annotations)
//! - /R - Rollover appearance (optional, for interactive elements)
//! - /D - Down appearance (optional, for clicked state)
//!
//! # Example
//!
//! ```ignore
//! use xberg_native_pdf::writer::AppearanceStreamBuilder;
//! use xberg_native_pdf::geometry::Rect;
//!
//! // Create a highlight appearance stream
//! let ap = AppearanceStreamBuilder::for_highlight(
//!     Rect::new(100.0, 700.0, 200.0, 20.0),
//!     AnnotationColor::yellow(),
//!     0.5,
//! );
//! let (dict, stream_bytes) = ap.build();
//! ```

use crate::annotation_types::{AnnotationColor, CaretSymbol, LineEndingStyle, StampType, TextAnnotationIcon};
use crate::geometry::Rect;
use crate::object::Object;
use std::collections::HashMap;

/// Builder for creating PDF appearance streams.
///
/// Appearance streams are Form XObjects that define the visual representation
/// of annotations. This builder generates the content stream bytes and the
/// Form XObject dictionary.
#[derive(Debug, Clone)]
pub struct AppearanceStreamBuilder {
    /// Bounding box for the appearance
    bbox: Rect,
    /// Content stream operations as bytes
    content: Vec<u8>,
    /// Resources needed by the appearance (fonts, etc.)
    resources: HashMap<String, Object>,
    /// Matrix transformation (optional)
    matrix: Option<[f64; 6]>,
}

impl AppearanceStreamBuilder {
    /// Create a new appearance stream builder with the given bounding box.
    pub fn new(bbox: Rect) -> Self {
        Self {
            bbox,
            content: Vec::new(),
            resources: HashMap::new(),
            matrix: None,
        }
    }

    /// Create an appearance stream for a highlight annotation.
    ///
    /// Renders a semi-transparent colored rectangle.
    pub fn for_highlight(rect: Rect, color: AnnotationColor, opacity: f32) -> Self {
        let mut builder = Self::new(Rect::new(0.0, 0.0, rect.width, rect.height));

        let mut content = String::new();

        if opacity < 1.0 {
            content.push_str("/GS1 gs\n");
            builder.add_ext_gstate("GS1", opacity);
        }

        if let Some(color_ops) = Self::color_to_fill_ops(&color) {
            content.push_str(&color_ops);
        }

        content.push_str(&format!("0 0 {} {} re f\n", rect.width, rect.height));

        builder.content = content.into_bytes();
        builder
    }

    /// Create an appearance stream for an underline annotation.
    pub fn for_underline(rect: Rect, color: AnnotationColor, opacity: f32) -> Self {
        let mut builder = Self::new(Rect::new(0.0, 0.0, rect.width, rect.height));

        let mut content = String::new();

        if opacity < 1.0 {
            content.push_str("/GS1 gs\n");
            builder.add_ext_gstate("GS1", opacity);
        }

        if let Some(color_ops) = Self::color_to_stroke_ops(&color) {
            content.push_str(&color_ops);
        }

        content.push_str("1 w\n");
        content.push_str(&format!("0 0 m {} 0 l S\n", rect.width));

        builder.content = content.into_bytes();
        builder
    }

    /// Create an appearance stream for a strikeout annotation.
    pub fn for_strikeout(rect: Rect, color: AnnotationColor, opacity: f32) -> Self {
        let mut builder = Self::new(Rect::new(0.0, 0.0, rect.width, rect.height));

        let mut content = String::new();

        if opacity < 1.0 {
            content.push_str("/GS1 gs\n");
            builder.add_ext_gstate("GS1", opacity);
        }

        if let Some(color_ops) = Self::color_to_stroke_ops(&color) {
            content.push_str(&color_ops);
        }

        let mid_y = rect.height / 2.0;
        content.push_str("1 w\n");
        content.push_str(&format!("0 {} m {} {} l S\n", mid_y, rect.width, mid_y));

        builder.content = content.into_bytes();
        builder
    }

    /// Create an appearance stream for a squiggly underline annotation.
    pub fn for_squiggly(rect: Rect, color: AnnotationColor, opacity: f32) -> Self {
        let mut builder = Self::new(Rect::new(0.0, 0.0, rect.width, rect.height));

        let mut content = String::new();

        if opacity < 1.0 {
            content.push_str("/GS1 gs\n");
            builder.add_ext_gstate("GS1", opacity);
        }

        if let Some(color_ops) = Self::color_to_stroke_ops(&color) {
            content.push_str(&color_ops);
        }

        content.push_str("0.5 w\n");
        let wave_height = 2.0_f32;
        let wave_length = 4.0_f32;
        let mut x = 0.0_f32;
        content.push_str(&format!("{} 0 m\n", x));

        while x < rect.width {
            let x1 = x + wave_length / 2.0;
            let x2 = x + wave_length;
            let y1 = wave_height;
            let y2 = 0.0;
            content.push_str(&format!("{} {} {} {} v\n", x1, y1, x2, y2));
            x = x2;
        }
        content.push_str("S\n");

        builder.content = content.into_bytes();
        builder
    }

    /// Create an appearance stream for a text (sticky note) annotation.
    ///
    /// Renders an icon based on the icon type.
    pub fn for_text_note(rect: Rect, icon: TextAnnotationIcon, color: AnnotationColor) -> Self {
        let size = rect.width.min(rect.height);
        let mut builder = Self::new(Rect::new(0.0, 0.0, size, size));

        let mut content = String::new();

        if let Some(color_ops) = Self::color_to_fill_ops(&color) {
            content.push_str(&color_ops);
        }

        match icon {
            TextAnnotationIcon::Note => {
                content.push_str(&Self::draw_note_icon(size));
            }
            TextAnnotationIcon::Comment => {
                content.push_str(&Self::draw_comment_icon(size));
            }
            TextAnnotationIcon::Key => {
                content.push_str(&Self::draw_key_icon(size));
            }
            TextAnnotationIcon::Help => {
                content.push_str(&Self::draw_help_icon(size));
            }
            TextAnnotationIcon::Insert => {
                content.push_str(&Self::draw_insert_icon(size));
            }
            TextAnnotationIcon::Paragraph | TextAnnotationIcon::NewParagraph => {
                content.push_str(&Self::draw_paragraph_icon(size));
            }
        }

        builder.content = content.into_bytes();
        builder
    }

    /// Create an appearance stream for a stamp annotation.
    pub fn for_stamp(rect: Rect, _stamp_type: StampType, color: AnnotationColor) -> Self {
        let mut builder = Self::new(Rect::new(0.0, 0.0, rect.width, rect.height));

        let mut content = String::new();

        if let Some(color_ops) = Self::color_to_stroke_ops(&color) {
            content.push_str(&color_ops);
        }

        let w = rect.width;
        let h = rect.height;
        let r = 5.0_f32.min(w / 6.0).min(h / 6.0);

        content.push_str("2 w\n");

        content.push_str(&format!("{} 0 m\n", r));
        content.push_str(&format!("{} 0 l\n", w - r));
        content.push_str(&format!("{} {} {} {} {} {} c\n", w - r, 0.0, w, 0.0, w, r));
        content.push_str(&format!("{} {} l\n", w, h - r));
        content.push_str(&format!("{} {} {} {} {} {} c\n", w, h - r, w, h, w - r, h));
        content.push_str(&format!("{} {} l\n", r, h));
        content.push_str(&format!("{} {} {} {} {} {} c\n", r, h, 0.0, h, 0.0, h - r));
        content.push_str(&format!("0 {} l\n", r));
        content.push_str(&format!("{} {} {} {} {} {} c\n", 0.0, r, 0.0, 0.0, r, 0.0));
        content.push_str("S\n");

        builder.content = content.into_bytes();
        builder
    }

    /// Create an appearance stream for a line annotation.
    pub fn for_line(
        start: (f64, f64),
        end: (f64, f64),
        color: AnnotationColor,
        width: f32,
        start_ending: LineEndingStyle,
        end_ending: LineEndingStyle,
    ) -> Self {
        let min_x = (start.0.min(end.0) - 10.0) as f32;
        let min_y = (start.1.min(end.1) - 10.0) as f32;
        let max_x = (start.0.max(end.0) + 10.0) as f32;
        let max_y = (start.1.max(end.1) + 10.0) as f32;

        let mut builder = Self::new(Rect::new(0.0, 0.0, max_x - min_x, max_y - min_y));

        let mut content = String::new();

        let x1 = start.0 as f32 - min_x;
        let y1 = start.1 as f32 - min_y;
        let x2 = end.0 as f32 - min_x;
        let y2 = end.1 as f32 - min_y;

        if let Some(color_ops) = Self::color_to_stroke_ops(&color) {
            content.push_str(&color_ops);
        }
        content.push_str(&format!("{} w\n", width));

        content.push_str(&format!("{} {} m {} {} l S\n", x1, y1, x2, y2));

        if start_ending != LineEndingStyle::None {
            content.push_str(&Self::draw_line_ending(x1, y1, x2, y2, start_ending, true));
        }
        if end_ending != LineEndingStyle::None {
            content.push_str(&Self::draw_line_ending(x1, y1, x2, y2, end_ending, false));
        }

        builder.content = content.into_bytes();
        builder.matrix = Some([1.0, 0.0, 0.0, 1.0, min_x as f64, min_y as f64]);
        builder
    }

    /// Create an appearance stream for a rectangle (Square) annotation.
    pub fn for_rectangle(
        rect: Rect,
        stroke_color: AnnotationColor,
        fill_color: Option<AnnotationColor>,
        width: f32,
    ) -> Self {
        let mut builder = Self::new(Rect::new(0.0, 0.0, rect.width, rect.height));

        let mut content = String::new();

        content.push_str(&format!("{} w\n", width));

        if let Some(color_ops) = Self::color_to_stroke_ops(&stroke_color) {
            content.push_str(&color_ops);
        }

        if let Some(ref fill) = fill_color
            && let Some(color_ops) = Self::color_to_fill_ops(fill)
        {
            content.push_str(&color_ops);
        }

        let offset = width / 2.0;
        content.push_str(&format!(
            "{} {} {} {} re ",
            offset,
            offset,
            rect.width - width,
            rect.height - width
        ));

        if fill_color.is_some() {
            content.push_str("B\n");
        } else {
            content.push_str("S\n");
        }

        builder.content = content.into_bytes();
        builder
    }

    /// Create an appearance stream for a circle annotation.
    pub fn for_circle(
        rect: Rect,
        stroke_color: AnnotationColor,
        fill_color: Option<AnnotationColor>,
        width: f32,
    ) -> Self {
        let mut builder = Self::new(Rect::new(0.0, 0.0, rect.width, rect.height));

        let mut content = String::new();

        content.push_str(&format!("{} w\n", width));

        if let Some(color_ops) = Self::color_to_stroke_ops(&stroke_color) {
            content.push_str(&color_ops);
        }

        if let Some(ref fill) = fill_color
            && let Some(color_ops) = Self::color_to_fill_ops(fill)
        {
            content.push_str(&color_ops);
        }

        let cx = rect.width / 2.0;
        let cy = rect.height / 2.0;
        let rx = (rect.width - width) / 2.0;
        let ry = (rect.height - width) / 2.0;

        // Magic number for Bézier approximation of circle ~keep
        let k = 0.552_284_7_f32;
        let kx = rx * k;
        let ky = ry * k;

        content.push_str(&format!("{} {} m\n", cx + rx, cy));
        content.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            cx + rx,
            cy + ky,
            cx + kx,
            cy + ry,
            cx,
            cy + ry
        ));
        content.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            cx - kx,
            cy + ry,
            cx - rx,
            cy + ky,
            cx - rx,
            cy
        ));
        content.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            cx - rx,
            cy - ky,
            cx - kx,
            cy - ry,
            cx,
            cy - ry
        ));
        content.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            cx + kx,
            cy - ry,
            cx + rx,
            cy - ky,
            cx + rx,
            cy
        ));

        if fill_color.is_some() {
            content.push_str("B\n");
        } else {
            content.push_str("S\n");
        }

        builder.content = content.into_bytes();
        builder
    }

    /// Create an appearance stream for an ink annotation.
    pub fn for_ink(strokes: &[Vec<(f64, f64)>], color: AnnotationColor, width: f32) -> Self {
        if strokes.is_empty() {
            return Self::new(Rect::new(0.0, 0.0, 1.0, 1.0));
        }

        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;

        for stroke in strokes {
            for (x, y) in stroke {
                min_x = min_x.min(*x);
                min_y = min_y.min(*y);
                max_x = max_x.max(*x);
                max_y = max_y.max(*y);
            }
        }

        let padding = width as f64 * 2.0;
        min_x -= padding;
        min_y -= padding;
        max_x += padding;
        max_y += padding;

        let mut builder = Self::new(Rect::new(0.0, 0.0, (max_x - min_x) as f32, (max_y - min_y) as f32));

        let mut content = String::new();

        if let Some(color_ops) = Self::color_to_stroke_ops(&color) {
            content.push_str(&color_ops);
        }
        content.push_str(&format!("{} w\n", width));
        content.push_str("1 J\n");
        content.push_str("1 j\n");

        for stroke in strokes {
            if stroke.is_empty() {
                continue;
            }

            let (x0, y0) = stroke[0];
            content.push_str(&format!("{} {} m\n", x0 - min_x, y0 - min_y));

            for (x, y) in stroke.iter().skip(1) {
                content.push_str(&format!("{} {} l\n", x - min_x, y - min_y));
            }
            content.push_str("S\n");
        }

        builder.content = content.into_bytes();
        builder.matrix = Some([1.0, 0.0, 0.0, 1.0, min_x, min_y]);
        builder
    }

    /// Create an appearance stream for a caret annotation.
    pub fn for_caret(rect: Rect, symbol: CaretSymbol, color: AnnotationColor) -> Self {
        let mut builder = Self::new(Rect::new(0.0, 0.0, rect.width, rect.height));

        let mut content = String::new();

        if let Some(color_ops) = Self::color_to_fill_ops(&color) {
            content.push_str(&color_ops);
        }

        match symbol {
            CaretSymbol::None => {
                let w = rect.width;
                let h = rect.height;
                content.push_str(&format!("0 0 m {} {} l {} 0 l S\n", w / 2.0, h, w));
            }
            CaretSymbol::Paragraph => {
                content.push_str(&Self::draw_paragraph_icon(rect.width.min(rect.height)));
            }
        }

        builder.content = content.into_bytes();
        builder
    }

    /// Create an appearance stream for a redact annotation.
    pub fn for_redact(rect: Rect, color: Option<AnnotationColor>) -> Self {
        let mut builder = Self::new(Rect::new(0.0, 0.0, rect.width, rect.height));

        let mut content = String::new();

        let fill_color = color.unwrap_or(AnnotationColor::black());

        if let Some(color_ops) = Self::color_to_fill_ops(&fill_color) {
            content.push_str(&color_ops);
        }

        content.push_str(&format!("0 0 {} {} re f\n", rect.width, rect.height));

        builder.content = content.into_bytes();
        builder
    }

    /// Add an extended graphics state resource.
    fn add_ext_gstate(&mut self, name: &str, opacity: f32) {
        let mut gs_dict = HashMap::new();
        gs_dict.insert("Type".to_string(), Object::Name("ExtGState".to_string()));
        gs_dict.insert("CA".to_string(), Object::Real(opacity as f64));
        gs_dict.insert("ca".to_string(), Object::Real(opacity as f64));

        let mut ext_gstate = self
            .resources
            .entry("ExtGState".to_string())
            .or_insert_with(|| Object::Dictionary(HashMap::new()))
            .clone();

        if let Object::Dictionary(ref mut dict) = ext_gstate {
            dict.insert(name.to_string(), Object::Dictionary(gs_dict));
        }
        self.resources.insert("ExtGState".to_string(), ext_gstate);
    }

    /// Convert color to fill operators.
    fn color_to_fill_ops(color: &AnnotationColor) -> Option<String> {
        match color {
            AnnotationColor::None => None,
            AnnotationColor::Gray(g) => Some(format!("{} g\n", g)),
            AnnotationColor::Rgb(r, g, b) => Some(format!("{} {} {} rg\n", r, g, b)),
            AnnotationColor::Cmyk(c, m, y, k) => Some(format!("{} {} {} {} k\n", c, m, y, k)),
        }
    }

    /// Convert color to stroke operators.
    fn color_to_stroke_ops(color: &AnnotationColor) -> Option<String> {
        match color {
            AnnotationColor::None => None,
            AnnotationColor::Gray(g) => Some(format!("{} G\n", g)),
            AnnotationColor::Rgb(r, g, b) => Some(format!("{} {} {} RG\n", r, g, b)),
            AnnotationColor::Cmyk(c, m, y, k) => Some(format!("{} {} {} {} K\n", c, m, y, k)),
        }
    }

    /// Draw a note icon.
    fn draw_note_icon(size: f32) -> String {
        let margin = size * 0.1;
        let w = size - 2.0 * margin;
        let h = size - 2.0 * margin;
        let fold = w * 0.25;

        let mut s = String::new();
        s.push_str(&format!("{} {} m\n", margin, margin));
        s.push_str(&format!("{} {} l\n", margin, margin + h));
        s.push_str(&format!("{} {} l\n", margin + w, margin + h));
        s.push_str(&format!("{} {} l\n", margin + w, margin + fold));
        s.push_str(&format!("{} {} l\n", margin + w - fold, margin));
        s.push_str("h B\n");

        s.push_str(&format!("{} {} m\n", margin + w - fold, margin));
        s.push_str(&format!("{} {} l\n", margin + w - fold, margin + fold));
        s.push_str(&format!("{} {} l S\n", margin + w, margin + fold));

        s
    }

    /// Draw a comment/speech bubble icon.
    fn draw_comment_icon(size: f32) -> String {
        let margin = size * 0.1;
        let w = size - 2.0 * margin;
        let h = (size - 2.0 * margin) * 0.8;
        let tail_h = (size - 2.0 * margin) * 0.2;

        let mut s = String::new();
        let r = size * 0.1;
        s.push_str(&format!("{} {} m\n", margin + r, margin + tail_h));
        s.push_str(&format!("{} {} l\n", margin + w - r, margin + tail_h));
        s.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            margin + w,
            margin + tail_h,
            margin + w,
            margin + tail_h + r,
            margin + w,
            margin + tail_h + r
        ));
        s.push_str(&format!("{} {} l\n", margin + w, margin + tail_h + h - r));
        s.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            margin + w,
            margin + tail_h + h,
            margin + w - r,
            margin + tail_h + h,
            margin + w - r,
            margin + tail_h + h
        ));
        s.push_str(&format!("{} {} l\n", margin + r, margin + tail_h + h));
        s.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            margin,
            margin + tail_h + h,
            margin,
            margin + tail_h + h - r,
            margin,
            margin + tail_h + h - r
        ));
        s.push_str(&format!("{} {} l\n", margin, margin + tail_h + r));
        s.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            margin,
            margin + tail_h,
            margin + r,
            margin + tail_h,
            margin + r,
            margin + tail_h
        ));
        s.push_str("h B\n");

        s.push_str(&format!("{} {} m\n", margin + w * 0.2, margin + tail_h));
        s.push_str(&format!("{} {} l\n", margin, margin));
        s.push_str(&format!("{} {} l f\n", margin + w * 0.4, margin + tail_h));

        s
    }

    /// Draw a key icon.
    fn draw_key_icon(size: f32) -> String {
        let cx = size / 2.0;
        let cy = size * 0.7;
        let r = size * 0.2;

        let mut s = String::new();

        let k = 0.552_284_7_f32 * r;
        s.push_str(&format!("{} {} m\n", cx + r, cy));
        s.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            cx + r,
            cy + k,
            cx + k,
            cy + r,
            cx,
            cy + r
        ));
        s.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            cx - k,
            cy + r,
            cx - r,
            cy + k,
            cx - r,
            cy
        ));
        s.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            cx - r,
            cy - k,
            cx - k,
            cy - r,
            cx,
            cy - r
        ));
        s.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            cx + k,
            cy - r,
            cx + r,
            cy - k,
            cx + r,
            cy
        ));
        s.push_str("S\n");

        let shaft_w = size * 0.08;
        s.push_str(&format!("{} {} m\n", cx - shaft_w / 2.0, cy - r));
        s.push_str(&format!("{} {} l\n", cx - shaft_w / 2.0, size * 0.15));
        s.push_str(&format!("{} {} l\n", cx + shaft_w / 2.0, size * 0.15));
        s.push_str(&format!("{} {} l h f\n", cx + shaft_w / 2.0, cy - r));

        let tooth_w = size * 0.1;
        s.push_str(&format!("{} {} m\n", cx + shaft_w / 2.0, size * 0.25));
        s.push_str(&format!("{} {} l\n", cx + shaft_w / 2.0 + tooth_w, size * 0.25));
        s.push_str(&format!("{} {} l\n", cx + shaft_w / 2.0 + tooth_w, size * 0.2));
        s.push_str(&format!("{} {} l h f\n", cx + shaft_w / 2.0, size * 0.2));

        s
    }

    /// Draw a help (question mark) icon.
    fn draw_help_icon(size: f32) -> String {
        let cx = size / 2.0;
        let mut s = String::new();

        s.push_str(&format!("{} {} m\n", cx - size * 0.15, size * 0.75));
        s.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            cx - size * 0.15,
            size * 0.9,
            cx + size * 0.15,
            size * 0.9,
            cx + size * 0.15,
            size * 0.7
        ));
        s.push_str(&format!(
            "{} {} {} {} {} {} c\n",
            cx + size * 0.15,
            size * 0.5,
            cx,
            size * 0.45,
            cx,
            size * 0.35
        ));
        s.push_str("S\n");

        let dot_r = size * 0.06;
        s.push_str(&format!(
            "{} {} {} {} re f\n",
            cx - dot_r,
            size * 0.15 - dot_r,
            dot_r * 2.0,
            dot_r * 2.0
        ));

        s
    }

    /// Draw an insert icon (caret).
    fn draw_insert_icon(size: f32) -> String {
        let cx = size / 2.0;
        let mut s = String::new();

        s.push_str(&format!("{} {} m\n", size * 0.2, size * 0.3));
        s.push_str(&format!("{} {} l\n", cx, size * 0.8));
        s.push_str(&format!("{} {} l S\n", size * 0.8, size * 0.3));

        s
    }

    /// Draw a paragraph icon.
    fn draw_paragraph_icon(size: f32) -> String {
        let mut s = String::new();

        let w = size * 0.6;
        let h = size * 0.8;
        let x = (size - w) / 2.0;
        let y = (size - h) / 2.0;

        s.push_str("1 w\n");
        s.push_str(&format!("{} {} m {} {} l S\n", x + w * 0.5, y, x + w * 0.5, y + h));
        s.push_str(&format!("{} {} m {} {} l S\n", x + w * 0.75, y, x + w * 0.75, y + h));

        let r = w * 0.3;
        s.push_str(&format!("{} {} m\n", x + w * 0.5, y + h));
        s.push_str(&format!("{} {} l\n", x + r, y + h));
        s.push_str(&format!(
            "{} {} {} {} {} {} c S\n",
            x,
            y + h,
            x,
            y + h - r * 2.0,
            x + w * 0.5,
            y + h - r * 2.0
        ));

        s
    }

    /// Draw a line ending.
    fn draw_line_ending(x1: f32, y1: f32, x2: f32, y2: f32, style: LineEndingStyle, at_start: bool) -> String {
        let size = 10.0_f32;

        let dx = x2 - x1;
        let dy = y2 - y1;
        let angle = dy.atan2(dx);

        let (px, py) = if at_start { (x1, y1) } else { (x2, y2) };

        let arrow_angle = if at_start { angle + std::f32::consts::PI } else { angle };

        let mut s = String::new();

        match style {
            LineEndingStyle::None => {}
            LineEndingStyle::OpenArrow => {
                let a1 = arrow_angle + std::f32::consts::PI / 6.0;
                let a2 = arrow_angle - std::f32::consts::PI / 6.0;
                s.push_str(&format!(
                    "{} {} m {} {} l S\n",
                    px + size * a1.cos(),
                    py + size * a1.sin(),
                    px,
                    py
                ));
                s.push_str(&format!(
                    "{} {} m {} {} l S\n",
                    px,
                    py,
                    px + size * a2.cos(),
                    py + size * a2.sin()
                ));
            }
            LineEndingStyle::ClosedArrow => {
                let a1 = arrow_angle + std::f32::consts::PI / 6.0;
                let a2 = arrow_angle - std::f32::consts::PI / 6.0;
                s.push_str(&format!(
                    "{} {} m {} {} l {} {} l h f\n",
                    px,
                    py,
                    px + size * a1.cos(),
                    py + size * a1.sin(),
                    px + size * a2.cos(),
                    py + size * a2.sin()
                ));
            }
            LineEndingStyle::Circle => {
                let r = size / 2.0;
                let k = 0.552_284_7_f32 * r;
                s.push_str(&format!("{} {} m\n", px + r, py));
                s.push_str(&format!(
                    "{} {} {} {} {} {} c\n",
                    px + r,
                    py + k,
                    px + k,
                    py + r,
                    px,
                    py + r
                ));
                s.push_str(&format!(
                    "{} {} {} {} {} {} c\n",
                    px - k,
                    py + r,
                    px - r,
                    py + k,
                    px - r,
                    py
                ));
                s.push_str(&format!(
                    "{} {} {} {} {} {} c\n",
                    px - r,
                    py - k,
                    px - k,
                    py - r,
                    px,
                    py - r
                ));
                s.push_str(&format!(
                    "{} {} {} {} {} {} c S\n",
                    px + k,
                    py - r,
                    px + r,
                    py - k,
                    px + r,
                    py
                ));
            }
            LineEndingStyle::Square => {
                let half = size / 2.0;
                s.push_str(&format!("{} {} {} {} re S\n", px - half, py - half, size, size));
            }
            LineEndingStyle::Diamond => {
                let half = size / 2.0;
                s.push_str(&format!("{} {} m\n", px, py - half));
                s.push_str(&format!("{} {} l\n", px + half, py));
                s.push_str(&format!("{} {} l\n", px, py + half));
                s.push_str(&format!("{} {} l h S\n", px - half, py));
            }
            _ => {}
        }

        s
    }

    /// Build the appearance stream, returning the Form XObject dictionary and content bytes.
    pub fn build(&self) -> (HashMap<String, Object>, Vec<u8>) {
        let mut dict = HashMap::new();

        dict.insert("Type".to_string(), Object::Name("XObject".to_string()));
        dict.insert("Subtype".to_string(), Object::Name("Form".to_string()));
        dict.insert("FormType".to_string(), Object::Integer(1));

        dict.insert(
            "BBox".to_string(),
            Object::Array(vec![
                Object::Real(self.bbox.x as f64),
                Object::Real(self.bbox.y as f64),
                Object::Real((self.bbox.x + self.bbox.width) as f64),
                Object::Real((self.bbox.y + self.bbox.height) as f64),
            ]),
        );

        if let Some(m) = self.matrix {
            dict.insert(
                "Matrix".to_string(),
                Object::Array(vec![
                    Object::Real(m[0]),
                    Object::Real(m[1]),
                    Object::Real(m[2]),
                    Object::Real(m[3]),
                    Object::Real(m[4]),
                    Object::Real(m[5]),
                ]),
            );
        }

        if !self.resources.is_empty() {
            dict.insert("Resources".to_string(), Object::Dictionary(self.resources.clone()));
        }

        // Length will be set by the caller when adding to PDF ~keep
        dict.insert("Length".to_string(), Object::Integer(self.content.len() as i64));

        (dict, self.content.clone())
    }

    /// Get the bounding box.
    pub fn bbox(&self) -> Rect {
        self.bbox
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highlight_appearance() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_highlight(rect, AnnotationColor::yellow(), 0.5);

        let (dict, content) = ap.build();

        assert!(dict.contains_key("Type"));
        assert!(dict.contains_key("Subtype"));
        assert!(dict.contains_key("BBox"));
        assert!(!content.is_empty());

        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("1 1 0 rg"));
        assert!(content_str.contains("re f"));
    }

    #[test]
    fn test_underline_appearance() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_underline(rect, AnnotationColor::green(), 1.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("0 1 0 RG"));
        assert!(content_str.contains("l S"));
    }

    #[test]
    fn test_strikeout_appearance() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_strikeout(rect, AnnotationColor::red(), 1.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("1 0 0 RG"));
        assert!(content_str.contains("10 m"));
    }

    #[test]
    fn test_text_note_appearance() {
        let rect = Rect::new(0.0, 0.0, 24.0, 24.0);
        let ap = AppearanceStreamBuilder::for_text_note(rect, TextAnnotationIcon::Note, AnnotationColor::yellow());

        let (dict, content) = ap.build();
        assert!(dict.contains_key("BBox"));
        assert!(!content.is_empty());
    }

    #[test]
    fn test_rectangle_appearance() {
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        let ap = AppearanceStreamBuilder::for_rectangle(
            rect,
            AnnotationColor::blue(),
            Some(AnnotationColor::Rgb(0.9, 0.9, 1.0)),
            2.0,
        );

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("0 0 1 RG"));
        assert!(content_str.contains("re B"));
    }

    #[test]
    fn test_circle_appearance() {
        let rect = Rect::new(0.0, 0.0, 50.0, 50.0);
        let ap = AppearanceStreamBuilder::for_circle(rect, AnnotationColor::red(), None, 1.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("1 0 0 RG"));
        assert!(content_str.contains("c"));
        assert!(content_str.contains("S"));
    }

    #[test]
    fn test_ink_appearance() {
        let strokes = vec![vec![(10.0, 10.0), (50.0, 50.0), (100.0, 10.0)]];
        let ap = AppearanceStreamBuilder::for_ink(&strokes, AnnotationColor::blue(), 2.0);

        let (dict, content) = ap.build();
        assert!(dict.contains_key("Matrix"));

        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("0 0 1 RG"));
        assert!(content_str.contains("m"));
        assert!(content_str.contains("l"));
        assert!(content_str.contains("S"));
    }

    #[test]
    fn test_line_appearance() {
        let ap = AppearanceStreamBuilder::for_line(
            (10.0, 10.0),
            (100.0, 50.0),
            AnnotationColor::black(),
            1.0,
            LineEndingStyle::None,
            LineEndingStyle::ClosedArrow,
        );

        let (dict, content) = ap.build();
        assert!(dict.contains_key("Matrix"));

        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("l S"));
        assert!(content_str.contains("h f"));
    }

    #[test]
    fn test_stamp_appearance() {
        let rect = Rect::new(0.0, 0.0, 150.0, 50.0);
        let ap = AppearanceStreamBuilder::for_stamp(rect, StampType::Approved, AnnotationColor::red());

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("1 0 0 RG"));
        assert!(content_str.contains("c"));
    }

    #[test]
    fn test_caret_appearance() {
        let rect = Rect::new(0.0, 0.0, 20.0, 20.0);
        let ap = AppearanceStreamBuilder::for_caret(rect, CaretSymbol::None, AnnotationColor::blue());

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("m"));
        assert!(content_str.contains("l"));
    }

    #[test]
    fn test_redact_appearance() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_redact(rect, None);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("0 g"));
        assert!(content_str.contains("re f"));
    }

    #[test]
    fn test_color_operators() {
        assert_eq!(
            AppearanceStreamBuilder::color_to_fill_ops(&AnnotationColor::yellow()),
            Some("1 1 0 rg\n".to_string())
        );
        assert_eq!(
            AppearanceStreamBuilder::color_to_stroke_ops(&AnnotationColor::blue()),
            Some("0 0 1 RG\n".to_string())
        );
        assert_eq!(
            AppearanceStreamBuilder::color_to_fill_ops(&AnnotationColor::Gray(0.5)),
            Some("0.5 g\n".to_string())
        );
        assert_eq!(AppearanceStreamBuilder::color_to_fill_ops(&AnnotationColor::None), None);
    }

    #[test]
    fn test_build_with_resources() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_highlight(rect, AnnotationColor::yellow(), 0.5);

        let (dict, _) = ap.build();

        assert!(dict.contains_key("Resources"));
    }

    #[test]
    fn test_new_builder() {
        let bbox = Rect::new(10.0, 20.0, 100.0, 50.0);
        let builder = AppearanceStreamBuilder::new(bbox);
        assert_eq!(builder.bbox().x, 10.0);
        assert_eq!(builder.bbox().y, 20.0);
        assert_eq!(builder.bbox().width, 100.0);
        assert_eq!(builder.bbox().height, 50.0);
    }

    #[test]
    fn test_new_builder_empty_content() {
        let builder = AppearanceStreamBuilder::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let (dict, content) = builder.build();
        assert!(content.is_empty());
        assert_eq!(dict.get("Length"), Some(&Object::Integer(0)));
    }

    #[test]
    fn test_build_dict_type() {
        let builder = AppearanceStreamBuilder::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let (dict, _) = builder.build();
        assert_eq!(dict.get("Type"), Some(&Object::Name("XObject".to_string())));
    }

    #[test]
    fn test_build_dict_subtype() {
        let builder = AppearanceStreamBuilder::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let (dict, _) = builder.build();
        assert_eq!(dict.get("Subtype"), Some(&Object::Name("Form".to_string())));
    }

    #[test]
    fn test_build_dict_form_type() {
        let builder = AppearanceStreamBuilder::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let (dict, _) = builder.build();
        assert_eq!(dict.get("FormType"), Some(&Object::Integer(1)));
    }

    #[test]
    fn test_build_dict_bbox() {
        let bbox = Rect::new(5.0, 10.0, 100.0, 200.0);
        let builder = AppearanceStreamBuilder::new(bbox);
        let (dict, _) = builder.build();
        let bbox_arr = dict.get("BBox").unwrap();
        if let Object::Array(arr) = bbox_arr {
            assert_eq!(arr.len(), 4);
            assert_eq!(arr[0], Object::Real(5.0));
            assert_eq!(arr[1], Object::Real(10.0));
            assert_eq!(arr[2], Object::Real(105.0));
            assert_eq!(arr[3], Object::Real(210.0));
        } else {
            panic!("BBox should be an Array");
        }
    }

    #[test]
    fn test_build_no_matrix_by_default() {
        let builder = AppearanceStreamBuilder::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let (dict, _) = builder.build();
        assert!(!dict.contains_key("Matrix"));
    }

    #[test]
    fn test_build_no_resources_by_default() {
        let builder = AppearanceStreamBuilder::new(Rect::new(0.0, 0.0, 50.0, 50.0));
        let (dict, _) = builder.build();
        assert!(!dict.contains_key("Resources"));
    }

    #[test]
    fn test_build_dict_length_matches_content() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_highlight(rect, AnnotationColor::red(), 1.0);
        let (dict, content) = ap.build();
        assert_eq!(dict.get("Length"), Some(&Object::Integer(content.len() as i64)));
    }

    #[test]
    fn test_color_fill_rgb() {
        assert_eq!(
            AppearanceStreamBuilder::color_to_fill_ops(&AnnotationColor::Rgb(0.5, 0.6, 0.7)),
            Some("0.5 0.6 0.7 rg\n".to_string())
        );
    }

    #[test]
    fn test_color_fill_cmyk() {
        assert_eq!(
            AppearanceStreamBuilder::color_to_fill_ops(&AnnotationColor::Cmyk(0.1, 0.2, 0.3, 0.4)),
            Some("0.1 0.2 0.3 0.4 k\n".to_string())
        );
    }

    #[test]
    fn test_color_stroke_gray() {
        assert_eq!(
            AppearanceStreamBuilder::color_to_stroke_ops(&AnnotationColor::Gray(0.75)),
            Some("0.75 G\n".to_string())
        );
    }

    #[test]
    fn test_color_stroke_rgb() {
        assert_eq!(
            AppearanceStreamBuilder::color_to_stroke_ops(&AnnotationColor::Rgb(1.0, 0.0, 0.5)),
            Some("1 0 0.5 RG\n".to_string())
        );
    }

    #[test]
    fn test_color_stroke_cmyk() {
        assert_eq!(
            AppearanceStreamBuilder::color_to_stroke_ops(&AnnotationColor::Cmyk(0.0, 1.0, 0.0, 0.0)),
            Some("0 1 0 0 K\n".to_string())
        );
    }

    #[test]
    fn test_color_stroke_none() {
        assert_eq!(
            AppearanceStreamBuilder::color_to_stroke_ops(&AnnotationColor::None),
            None
        );
    }

    #[test]
    fn test_highlight_full_opacity() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_highlight(rect, AnnotationColor::yellow(), 1.0);

        let (dict, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(!content_str.contains("/GS1 gs"));
        assert!(!dict.contains_key("Resources"));
        assert!(content_str.contains("1 1 0 rg"));
        assert!(content_str.contains("re f"));
    }

    #[test]
    fn test_highlight_partial_opacity() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_highlight(rect, AnnotationColor::yellow(), 0.3);

        let (dict, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("/GS1 gs"));
        assert!(dict.contains_key("Resources"));
    }

    #[test]
    fn test_highlight_no_color() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_highlight(rect, AnnotationColor::None, 0.5);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("re f"));
        assert!(!content_str.contains("rg"));
    }

    #[test]
    fn test_underline_full_opacity() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_underline(rect, AnnotationColor::red(), 1.0);

        let (dict, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(!content_str.contains("/GS1 gs"));
        assert!(!dict.contains_key("Resources"));
        assert!(content_str.contains("1 0 0 RG"));
        assert!(content_str.contains("1 w"));
        assert!(content_str.contains("0 0 m"));
    }

    #[test]
    fn test_underline_partial_opacity() {
        let rect = Rect::new(0.0, 0.0, 200.0, 15.0);
        let ap = AppearanceStreamBuilder::for_underline(rect, AnnotationColor::blue(), 0.7);

        let (dict, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("/GS1 gs"));
        assert!(dict.contains_key("Resources"));
    }

    #[test]
    fn test_strikeout_midline_position() {
        let rect = Rect::new(0.0, 0.0, 100.0, 30.0);
        let ap = AppearanceStreamBuilder::for_strikeout(rect, AnnotationColor::red(), 1.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("15 m"));
        assert!(content_str.contains("100 15 l S"));
    }

    #[test]
    fn test_strikeout_partial_opacity() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_strikeout(rect, AnnotationColor::red(), 0.5);

        let (dict, _) = ap.build();
        assert!(dict.contains_key("Resources"));
    }

    #[test]
    fn test_squiggly_wave_pattern() {
        let rect = Rect::new(0.0, 0.0, 20.0, 10.0);
        let ap = AppearanceStreamBuilder::for_squiggly(rect, AnnotationColor::red(), 1.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("0.5 w"));
        assert!(content_str.contains("0 0 m"));
        assert!(content_str.contains("v"));
        assert!(content_str.contains("S"));
    }

    #[test]
    fn test_squiggly_partial_opacity() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_squiggly(rect, AnnotationColor::green(), 0.6);

        let (dict, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("/GS1 gs"));
        assert!(dict.contains_key("Resources"));
    }

    #[test]
    fn test_squiggly_zero_width() {
        let rect = Rect::new(0.0, 0.0, 0.0, 10.0);
        let ap = AppearanceStreamBuilder::for_squiggly(rect, AnnotationColor::red(), 1.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("S"));
    }

    #[test]
    fn test_text_note_comment_icon() {
        let rect = Rect::new(0.0, 0.0, 24.0, 24.0);
        let ap = AppearanceStreamBuilder::for_text_note(rect, TextAnnotationIcon::Comment, AnnotationColor::yellow());
        let (_, content) = ap.build();
        assert!(!content.is_empty());
        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("h B"));
    }

    #[test]
    fn test_text_note_key_icon() {
        let rect = Rect::new(0.0, 0.0, 24.0, 24.0);
        let ap =
            AppearanceStreamBuilder::for_text_note(rect, TextAnnotationIcon::Key, AnnotationColor::Rgb(0.5, 0.5, 0.5));
        let (_, content) = ap.build();
        assert!(!content.is_empty());
        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("c"));
        assert!(content_str.contains("S"));
    }

    #[test]
    fn test_text_note_help_icon() {
        let rect = Rect::new(0.0, 0.0, 24.0, 24.0);
        let ap = AppearanceStreamBuilder::for_text_note(rect, TextAnnotationIcon::Help, AnnotationColor::blue());
        let (_, content) = ap.build();
        assert!(!content.is_empty());
        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("c"));
        assert!(content_str.contains("re f"));
    }

    #[test]
    fn test_text_note_insert_icon() {
        let rect = Rect::new(0.0, 0.0, 24.0, 24.0);
        let ap = AppearanceStreamBuilder::for_text_note(rect, TextAnnotationIcon::Insert, AnnotationColor::green());
        let (_, content) = ap.build();
        assert!(!content.is_empty());
        let content_str = String::from_utf8_lossy(&content);
        assert!(content_str.contains("m"));
        assert!(content_str.contains("l S"));
    }

    #[test]
    fn test_text_note_paragraph_icon() {
        let rect = Rect::new(0.0, 0.0, 24.0, 24.0);
        let ap = AppearanceStreamBuilder::for_text_note(rect, TextAnnotationIcon::Paragraph, AnnotationColor::red());
        let (_, content) = ap.build();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_text_note_new_paragraph_icon() {
        let rect = Rect::new(0.0, 0.0, 24.0, 24.0);
        let ap = AppearanceStreamBuilder::for_text_note(rect, TextAnnotationIcon::NewParagraph, AnnotationColor::red());
        let (_, content) = ap.build();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_text_note_uses_min_dimension() {
        let rect = Rect::new(0.0, 0.0, 20.0, 40.0);
        let ap = AppearanceStreamBuilder::for_text_note(rect, TextAnnotationIcon::Note, AnnotationColor::yellow());
        assert_eq!(ap.bbox().width, 20.0);
        assert_eq!(ap.bbox().height, 20.0);
    }

    #[test]
    fn test_text_note_no_color() {
        let rect = Rect::new(0.0, 0.0, 24.0, 24.0);
        let ap = AppearanceStreamBuilder::for_text_note(rect, TextAnnotationIcon::Note, AnnotationColor::None);
        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);
        assert!(!content_str.contains("rg"));
        assert!(!content_str.contains(" g\n"));
    }

    #[test]
    fn test_stamp_rounded_corners() {
        let rect = Rect::new(0.0, 0.0, 120.0, 40.0);
        let ap = AppearanceStreamBuilder::for_stamp(rect, StampType::Confidential, AnnotationColor::Rgb(0.8, 0.0, 0.0));
        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("2 w"));
        assert!(content_str.contains("c"));
        assert!(content_str.contains("l"));
        assert!(content_str.contains("S"));
    }

    #[test]
    fn test_stamp_small_rect() {
        let rect = Rect::new(0.0, 0.0, 12.0, 6.0);
        let ap = AppearanceStreamBuilder::for_stamp(rect, StampType::Approved, AnnotationColor::green());
        let (_, content) = ap.build();
        assert!(!content.is_empty());
    }

    #[test]
    fn test_line_with_open_arrow() {
        let ap = AppearanceStreamBuilder::for_line(
            (20.0, 20.0),
            (100.0, 80.0),
            AnnotationColor::black(),
            1.5,
            LineEndingStyle::OpenArrow,
            LineEndingStyle::None,
        );

        let (dict, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(dict.contains_key("Matrix"));
        assert!(content_str.contains("l S"));
    }

    #[test]
    fn test_line_with_circle_ending() {
        let ap = AppearanceStreamBuilder::for_line(
            (10.0, 10.0),
            (200.0, 10.0),
            AnnotationColor::blue(),
            2.0,
            LineEndingStyle::None,
            LineEndingStyle::Circle,
        );

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("c"));
    }

    #[test]
    fn test_line_with_square_ending() {
        let ap = AppearanceStreamBuilder::for_line(
            (10.0, 10.0),
            (200.0, 10.0),
            AnnotationColor::red(),
            1.0,
            LineEndingStyle::Square,
            LineEndingStyle::None,
        );

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("re S"));
    }

    #[test]
    fn test_line_with_diamond_ending() {
        let ap = AppearanceStreamBuilder::for_line(
            (10.0, 10.0),
            (200.0, 10.0),
            AnnotationColor::green(),
            1.0,
            LineEndingStyle::None,
            LineEndingStyle::Diamond,
        );

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("h S"));
    }

    #[test]
    fn test_line_both_endings() {
        let ap = AppearanceStreamBuilder::for_line(
            (0.0, 0.0),
            (100.0, 100.0),
            AnnotationColor::black(),
            1.0,
            LineEndingStyle::ClosedArrow,
            LineEndingStyle::ClosedArrow,
        );

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);
        let count = content_str.matches("h f").count();
        assert_eq!(count, 2);
    }

    #[test]
    fn test_line_matrix_offset() {
        let ap = AppearanceStreamBuilder::for_line(
            (50.0, 50.0),
            (150.0, 150.0),
            AnnotationColor::black(),
            1.0,
            LineEndingStyle::None,
            LineEndingStyle::None,
        );

        let (dict, _) = ap.build();
        assert!(dict.contains_key("Matrix"));
        if let Some(Object::Array(m)) = dict.get("Matrix") {
            assert_eq!(m.len(), 6);
            assert_eq!(m[0], Object::Real(1.0));
            assert_eq!(m[1], Object::Real(0.0));
        }
    }

    #[test]
    fn test_rectangle_stroke_only() {
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        let ap = AppearanceStreamBuilder::for_rectangle(rect, AnnotationColor::red(), None, 2.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("2 w"));
        assert!(content_str.contains("re S"));
    }

    #[test]
    fn test_rectangle_fill_and_stroke() {
        let rect = Rect::new(0.0, 0.0, 100.0, 50.0);
        let ap = AppearanceStreamBuilder::for_rectangle(
            rect,
            AnnotationColor::red(),
            Some(AnnotationColor::Rgb(0.9, 0.9, 0.9)),
            1.0,
        );

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("re B"));
        assert!(content_str.contains("rg"));
        assert!(content_str.contains("RG"));
    }

    #[test]
    fn test_circle_stroke_only() {
        let rect = Rect::new(0.0, 0.0, 50.0, 50.0);
        let ap = AppearanceStreamBuilder::for_circle(rect, AnnotationColor::blue(), None, 1.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.ends_with("S\n"));
        assert!(!content_str.contains("B\n"));
    }

    #[test]
    fn test_circle_fill_and_stroke() {
        let rect = Rect::new(0.0, 0.0, 80.0, 60.0);
        let ap = AppearanceStreamBuilder::for_circle(
            rect,
            AnnotationColor::black(),
            Some(AnnotationColor::Rgb(1.0, 1.0, 0.0)),
            2.0,
        );

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("B\n"));
        assert!(content_str.contains("c"));
    }

    #[test]
    fn test_circle_four_bezier_segments() {
        let rect = Rect::new(0.0, 0.0, 100.0, 100.0);
        let ap = AppearanceStreamBuilder::for_circle(rect, AnnotationColor::red(), None, 1.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert_eq!(content_str.matches(" c\n").count(), 4);
    }

    #[test]
    fn test_ink_empty_strokes() {
        let strokes: Vec<Vec<(f64, f64)>> = vec![];
        let ap = AppearanceStreamBuilder::for_ink(&strokes, AnnotationColor::black(), 1.0);

        assert_eq!(ap.bbox().width, 1.0);
        assert_eq!(ap.bbox().height, 1.0);
    }

    #[test]
    fn test_ink_single_point_stroke() {
        let strokes = vec![vec![(50.0, 50.0)]];
        let ap = AppearanceStreamBuilder::for_ink(&strokes, AnnotationColor::blue(), 2.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("m\n"));
        assert!(content_str.contains("S\n"));
    }

    #[test]
    fn test_ink_multiple_strokes() {
        let strokes = vec![vec![(10.0, 10.0), (50.0, 50.0)], vec![(60.0, 60.0), (100.0, 100.0)]];
        let ap = AppearanceStreamBuilder::for_ink(&strokes, AnnotationColor::red(), 1.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert_eq!(content_str.matches("S\n").count(), 2);
        assert_eq!(content_str.matches("m\n").count(), 2);
    }

    #[test]
    fn test_ink_stroke_with_empty_subpath() {
        let strokes = vec![vec![], vec![(10.0, 10.0), (50.0, 50.0)]];
        let ap = AppearanceStreamBuilder::for_ink(&strokes, AnnotationColor::blue(), 1.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert_eq!(content_str.matches("S\n").count(), 1);
    }

    #[test]
    fn test_ink_line_cap_and_join() {
        let strokes = vec![vec![(0.0, 0.0), (100.0, 100.0)]];
        let ap = AppearanceStreamBuilder::for_ink(&strokes, AnnotationColor::black(), 3.0);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("1 J"));
        assert!(content_str.contains("1 j"));
        assert!(content_str.contains("3 w"));
    }

    #[test]
    fn test_caret_none_symbol() {
        let rect = Rect::new(0.0, 0.0, 20.0, 30.0);
        let ap = AppearanceStreamBuilder::for_caret(rect, CaretSymbol::None, AnnotationColor::blue());

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("0 0 m"));
    }

    #[test]
    fn test_caret_paragraph_symbol() {
        let rect = Rect::new(0.0, 0.0, 20.0, 20.0);
        let ap = AppearanceStreamBuilder::for_caret(rect, CaretSymbol::Paragraph, AnnotationColor::black());

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("1 w"));
    }

    #[test]
    fn test_redact_default_black() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_redact(rect, None);

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("0 g"));
        assert!(content_str.contains("re f"));
    }

    #[test]
    fn test_redact_custom_color() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_redact(rect, Some(AnnotationColor::Rgb(0.5, 0.5, 0.5)));

        let (_, content) = ap.build();
        let content_str = String::from_utf8_lossy(&content);

        assert!(content_str.contains("0.5 0.5 0.5 rg"));
        assert!(content_str.contains("re f"));
    }

    #[test]
    fn test_draw_note_icon() {
        let s = AppearanceStreamBuilder::draw_note_icon(24.0);
        assert!(!s.is_empty());
        assert!(s.contains("h B"));
        assert!(s.contains("l S"));
    }

    #[test]
    fn test_draw_comment_icon() {
        let s = AppearanceStreamBuilder::draw_comment_icon(24.0);
        assert!(!s.is_empty());
        assert!(s.contains("h B"));
        assert!(s.contains("l f"));
    }

    #[test]
    fn test_draw_key_icon() {
        let s = AppearanceStreamBuilder::draw_key_icon(24.0);
        assert!(!s.is_empty());
        assert!(s.contains("S"));
        assert!(s.contains("h f"));
    }

    #[test]
    fn test_draw_help_icon() {
        let s = AppearanceStreamBuilder::draw_help_icon(24.0);
        assert!(!s.is_empty());
        assert!(s.contains("c"));
        assert!(s.contains("re f"));
    }

    #[test]
    fn test_draw_insert_icon() {
        let s = AppearanceStreamBuilder::draw_insert_icon(24.0);
        assert!(!s.is_empty());
        assert!(s.contains("m"));
        assert!(s.contains("l S"));
    }

    #[test]
    fn test_draw_paragraph_icon() {
        let s = AppearanceStreamBuilder::draw_paragraph_icon(24.0);
        assert!(!s.is_empty());
        assert!(s.contains("1 w"));
        assert!(s.contains("l S"));
        assert!(s.contains("c S"));
    }

    #[test]
    fn test_draw_line_ending_none() {
        let s = AppearanceStreamBuilder::draw_line_ending(0.0, 0.0, 100.0, 0.0, LineEndingStyle::None, false);
        assert!(s.is_empty());
    }

    #[test]
    fn test_draw_line_ending_open_arrow() {
        let s = AppearanceStreamBuilder::draw_line_ending(0.0, 0.0, 100.0, 0.0, LineEndingStyle::OpenArrow, false);
        assert!(!s.is_empty());
        assert!(s.contains("l S"));
    }

    #[test]
    fn test_draw_line_ending_closed_arrow() {
        let s = AppearanceStreamBuilder::draw_line_ending(0.0, 0.0, 100.0, 0.0, LineEndingStyle::ClosedArrow, false);
        assert!(!s.is_empty());
        assert!(s.contains("h f"));
    }

    #[test]
    fn test_draw_line_ending_circle() {
        let s = AppearanceStreamBuilder::draw_line_ending(0.0, 0.0, 100.0, 0.0, LineEndingStyle::Circle, false);
        assert!(!s.is_empty());
        assert!(s.contains("c S"));
    }

    #[test]
    fn test_draw_line_ending_square() {
        let s = AppearanceStreamBuilder::draw_line_ending(0.0, 0.0, 100.0, 0.0, LineEndingStyle::Square, false);
        assert!(!s.is_empty());
        assert!(s.contains("re S"));
    }

    #[test]
    fn test_draw_line_ending_diamond() {
        let s = AppearanceStreamBuilder::draw_line_ending(0.0, 0.0, 100.0, 0.0, LineEndingStyle::Diamond, false);
        assert!(!s.is_empty());
        assert!(s.contains("h S"));
    }

    #[test]
    fn test_draw_line_ending_at_start() {
        let s_start = AppearanceStreamBuilder::draw_line_ending(0.0, 0.0, 100.0, 0.0, LineEndingStyle::OpenArrow, true);
        let s_end = AppearanceStreamBuilder::draw_line_ending(0.0, 0.0, 100.0, 0.0, LineEndingStyle::OpenArrow, false);
        assert_ne!(s_start, s_end);
    }

    #[test]
    fn test_draw_line_ending_butt_fallthrough() {
        let s = AppearanceStreamBuilder::draw_line_ending(0.0, 0.0, 100.0, 0.0, LineEndingStyle::Butt, false);
        assert!(s.is_empty());
    }

    #[test]
    fn test_ext_gstate_opacity_values() {
        let rect = Rect::new(0.0, 0.0, 100.0, 20.0);
        let ap = AppearanceStreamBuilder::for_highlight(rect, AnnotationColor::yellow(), 0.3);

        let (dict, _) = ap.build();
        if let Some(Object::Dictionary(resources)) = dict.get("Resources") {
            if let Some(Object::Dictionary(ext_gstate)) = resources.get("ExtGState") {
                if let Some(Object::Dictionary(gs1)) = ext_gstate.get("GS1") {
                    if let Some(Object::Real(ca)) = gs1.get("CA") {
                        assert!((*ca - 0.3).abs() < 0.01);
                    }
                    if let Some(Object::Real(ca)) = gs1.get("ca") {
                        assert!((*ca - 0.3).abs() < 0.01);
                    }
                    assert_eq!(gs1.get("Type"), Some(&Object::Name("ExtGState".to_string())));
                } else {
                    panic!("GS1 not found in ExtGState");
                }
            } else {
                panic!("ExtGState not found in Resources");
            }
        } else {
            panic!("Resources not found");
        }
    }

    #[test]
    fn test_bbox_accessor() {
        let rect = Rect::new(5.0, 10.0, 200.0, 100.0);
        let ap = AppearanceStreamBuilder::for_highlight(rect, AnnotationColor::red(), 1.0);
        assert_eq!(ap.bbox().x, 0.0);
        assert_eq!(ap.bbox().y, 0.0);
        assert_eq!(ap.bbox().width, 200.0);
        assert_eq!(ap.bbox().height, 100.0);
    }
}
