//! TJ-array buffering and displacement handling.
//!
//! Split out of the parent's single 5,806-line `impl TextExtractor`, which made
//! `extractors/text.rs` 673 KiB — over the repository's 500 KiB file-safety limit.
//! A child module's `impl` is the same inherent impl and sees the parent's private
//! items unchanged. ~keep

use super::*;

/// Text-state snapshot needed to size and advance one `TJ` show-string's
/// glyphs, captured once per array so per-character work doesn't re-borrow
/// `self.state_stack`.
struct TjCharacterMetrics {
    font_size: f32,
    horizontal_scaling: f32,
    char_space: f32,
    word_space: f32,
}

impl<'doc> TextExtractor<'doc> {
    /// Get the current artifact type from the marked content stack.
    pub(super) fn current_artifact_type(&self) -> Option<ArtifactType> {
        self.marked_content_stack
            .iter()
            .rev()
            .find_map(|ctx| ctx.artifact_type.clone())
    }

    /// Flush accumulated TJ buffer into a single TextSpan.
    ///
    /// This creates one span for the entire buffer content, properly calculating
    /// the total width including character spacing (Tc) and word spacing (Tw).
    pub(super) fn flush_tj_buffer(&mut self, mut buffer: TjBuffer) -> Result<()> {
        if buffer.is_empty() {
            return Ok(());
        }

        let total_width = buffer.accumulated_width * buffer.user_h_scale;

        // Use pre-computed values from buffer creation (avoids
        // matrix multiply + sqrt + HashMap lookup + transform_point per flush) ~keep
        let effective_font_size = buffer.effective_font_size;
        let font_weight = buffer.font_weight;
        let is_italic_span = buffer.is_italic;

        // Move owned strings out of buffer (avoids clone) ~keep
        let font_name_span = buffer.font_name.take().unwrap_or_else(|| "Unknown".to_string());

        // RTL text correction. See `correct_tj_buffer_rtl_order` for the full
        // rationale (unchanged, just split out for file size). ~keep
        let text = std::mem::take(&mut buffer.unicode);
        let text = Self::correct_tj_buffer_rtl_order(text, &buffer);

        let span = TextSpan {
            provenance: None,
            text,
            bbox: Rect {
                x: buffer.user_pos_x,
                y: buffer.user_pos_y,
                width: total_width,
                height: effective_font_size,
            },
            font_name: font_name_span,
            font_size: effective_font_size,
            font_weight,
            color: Color::new(
                buffer.fill_color_rgb.0,
                buffer.fill_color_rgb.1,
                buffer.fill_color_rgb.2,
            ),
            mcid: buffer.mcid,
            mcid_scope: Some(self.current_mcid_scope()),
            sequence: self.span_sequence_counter,
            split_boundary_before: false,
            offset_semantic: false,
            char_spacing: buffer.char_space, // Tc - captured from PDF content stream ~keep
            word_spacing: buffer.word_space, // Tw - captured from PDF content stream ~keep
            horizontal_scaling: buffer.horizontal_scaling,
            // ~keep
            is_italic: is_italic_span,
            is_monospace: buffer.is_monospace,
            primary_detected: false,
            artifact_type: self.current_artifact_type(),
            char_widths: {
                let mut cw = std::mem::take(&mut buffer.char_widths);
                let h = buffer.user_h_scale;
                for w in &mut cw {
                    *w *= h;
                }
                cw
            },
            char_x_offsets: Vec::new(),
            heading_level: None,
            rotation_degrees: buffer.rotation_degrees,
            wmode: buffer.wmode,
            text_rise: buffer.text_rise,
            rtl_draw_logical: false,
            mirrored: buffer.mirrored,
            page_rotation_applied: 0,
        };
        self.span_sequence_counter += 1;

        if !self.is_content_suppressed() {
            self.spans.push(span);
        }
        Ok(())
    }

    /// RTL text correction for a flushed TJ buffer: use the confidence-gated
    /// geometric detector when `char_widths` gives us per-character
    /// user-space x-positions, falling back to the coarse "buffer's net
    /// horizontal advance is positive" heuristic only for genuinely
    /// ambiguous/short runs. Mirrors `flush_tj_span_buffer`'s handling — this
    /// used to be the one flush site still on the older
    /// `accumulated_width > 0.0` check, which (since `accumulated_width`
    /// only ever sums *positive* glyph widths — TJ kerning offsets never
    /// subtract from it) is true for nearly every non-empty RTL buffer and
    /// so was unconditionally reversing every RTL run regardless of its
    /// actual source order. ~keep
    fn correct_tj_buffer_rtl_order(text: String, buffer: &TjBuffer) -> String {
        if text.len() <= 1 {
            return text;
        }
        let has_rtl = text.chars().any(|c| crate::text::rtl_detector::is_rtl_text(c as u32));
        if !has_rtl {
            return text;
        }
        let chars: Vec<char> = text.chars().collect();
        let verdict = if chars.len() == buffer.char_widths.len() && !buffer.char_widths.is_empty() {
            let mut chars_with_x: Vec<(char, f32)> = Vec::with_capacity(chars.len());
            let mut cursor_text_space = 0.0_f32;
            for (i, c) in chars.iter().enumerate() {
                let user_x = buffer.user_pos_x + cursor_text_space * buffer.user_h_scale;
                chars_with_x.push((*c, user_x));
                cursor_text_space += buffer.char_widths[i];
            }
            crate::text::bidi::detect_visual_order_run(&chars_with_x)
        } else {
            crate::text::bidi::RunOrder::Ambiguous
        };
        crate::text::bidi::apply_rtl_verdict(
            &text,
            verdict,
            buffer.accumulated_width > 0.0,
            matches!(buffer.render_mode, 3 | 7),
        )
    }

    /// Calculate total width of TJ buffer using PDF spec formula.
    ///
    /// Process TJ array according to configured word boundary detection mode.
    ///
    /// Per PDF Spec ISO 32000-1:2008 Section 9.4.4,
    /// this method dispatches to either:
    /// - process_tj_array_tiebreaker(): WordBoundaryMode::Tiebreaker (default)
    /// - process_tj_array_primary(): WordBoundaryMode::Primary
    pub(super) fn process_tj_array(&mut self, array: &[TextElement]) -> Result<()> {
        // A bare `Tj` immediately before this array leaves `self.tj_span_buffer` open: `Tj`
        // buffers into that field, while `TJ` runs through its own local buffer below and never
        // reads or clears it. Every other run-closing boundary -- Td, TD, T*, a non-continuing
        // Tm, BMC/BDC, ' and ", ET -- already flushes it first; TJ was the sole omission. Left
        // open, the buffer keeps the origin captured at its first `Tj`, so a later `Tj` appends
        // onto it and the eventual flush emits the combined run at that stale x. The
        // reading-order sort then splices it in beside whatever sits near that stale position,
        // scrambling the line (GH#1544). ~keep
        self.flush_tj_span_buffer()?;
        match self.word_boundary_mode {
            WordBoundaryMode::Tiebreaker => self.process_tj_array_tiebreaker(array),
            WordBoundaryMode::Primary => self.process_tj_array_primary(array),
        }
    }

    pub(super) fn has_following_tj_displacement(array: &[TextElement], index: usize) -> bool {
        matches!(array.get(index + 1), Some(TextElement::Offset(offset)) if *offset != 0.0)
    }

    /// Process TJ array using tiebreaker mode (backward compatible).
    ///
    /// This is the legacy code path used when
    /// WordBoundaryMode::Tiebreaker is configured.
    ///
    /// Maintains 100% backward compatibility with existing behavior.
    /// Word boundaries are detected only as a tiebreaker when TJ offset
    /// and geometric signals contradict each other.
    ///
    /// Per PDF Spec ISO 32000-1:2008, Section 9.4.4 NOTE 6:
    /// "The performance of text searching (and other text extraction operations) is
    /// significantly better if the text strings are as long as possible."
    ///
    /// This method buffers consecutive strings into a single span, only breaking on:
    /// - Large negative offsets (indicating word boundaries)
    /// - End of TJ array
    fn process_tj_array_tiebreaker(&mut self, array: &[TextElement]) -> Result<()> {
        // Character-level tracking for word boundary detection
        // Collect detailed character information during TJ array processing
        // Per ISO 32000-1:2008 Section 9.4.4, character-level data improves accuracy ~keep

        self.tj_character_array.clear();
        self.current_x_position = 0.0;

        // Copy state data to avoid holding reference while borrowing self mutably ~keep
        let font_size = self.state_stack.current().font_size;
        let horizontal_scaling = self.state_stack.current().horizontal_scaling / 100.0;
        let font_name = self.state_stack.current().font_name.clone();
        let char_space = self.state_stack.current().char_space;
        let word_space = self.state_stack.current().word_space;

        let mut buffer = TjBuffer::new(
            self.state_stack.current(),
            self.current_mcid,
            self.cached_current_font.clone(),
        );
        let mut _element_count = 0;
        let tracking = self.uniform_tj_tracking(array, font_name.as_deref());

        for (idx, element) in array.iter().enumerate() {
            _element_count += 1;
            match element {
                TextElement::String(s) => {
                    let metrics = TjCharacterMetrics {
                        font_size,
                        horizontal_scaling,
                        char_space,
                        word_space,
                    };
                    self.process_tj_string_element(array, idx, s, &metrics, font_name.as_deref(), &mut buffer)?;
                }
                TextElement::Offset(offset) => {
                    buffer = self.process_tj_offset_element(array, idx, *offset, tracking, buffer)?;
                }
            }
        }

        if !buffer.is_empty() {
            self.flush_tj_buffer(buffer)?;
        }

        Ok(())
    }

    /// Track per-character info for one `TJ` show-string element and append
    /// it to the accumulating buffer. Split out of
    /// [`Self::process_tj_array_tiebreaker`] to keep it short. ~keep
    fn process_tj_string_element(
        &mut self,
        array: &[TextElement],
        idx: usize,
        s: &[u8],
        metrics: &TjCharacterMetrics,
        font_name: Option<&str>,
        buffer: &mut TjBuffer,
    ) -> Result<()> {
        if let Some(name) = font_name
            && let Some(font) = self.fonts.get(name)
        {
            let width_table = Self::simple_widths(
                self.cached_extraction_widths.as_deref(),
                font,
                !Self::has_following_tj_displacement(array, idx),
            );
            for &byte in s.iter() {
                // Normalize character code through encoding.
                // This ensures word boundary detection works on actual characters,
                // not raw byte codes from custom encodings ~keep
                let char_code = font.get_encoded_char(byte).map(|ch| ch as u32).unwrap_or(byte as u32);

                let glyph_width = width_table[byte as usize];

                let is_ligature = Self::is_ligature_code(char_code);

                // Create CharacterInfo for this character
                // The tj_offset will be applied when we encounter the next Offset element
                // ~keep
                let char_info = CharacterInfo {
                    code: char_code,
                    glyph_id: None, // Could be enhanced to extract actual GID ~keep
                    width: glyph_width,
                    x_position: self.current_x_position,
                    tj_offset: None,
                    font_size: metrics.font_size,
                    is_ligature,
                    original_ligature: None,
                    protected_from_split: false,
                };

                self.tj_character_array.push(char_info);

                let char_advance = glyph_width * metrics.horizontal_scaling
                    + metrics.char_space
                    + (if byte == 0x20 { metrics.word_space } else { 0.0 });
                self.current_x_position += char_advance;
            }
        }

        let repair_zero_widths = !Self::has_following_tj_displacement(array, idx);
        self.append_advance_buffer(buffer, s, repair_zero_widths)
    }

    /// Apply one `TJ` numeric offset: update the running statistics, decide
    /// whether it is a word boundary, and either flush `buffer` into a span
    /// (returning a fresh one) or fold the offset into it. Split out of
    /// [`Self::process_tj_array_tiebreaker`] to keep it short. ~keep
    fn process_tj_offset_element(
        &mut self,
        array: &[TextElement],
        idx: usize,
        offset: f32,
        tracking: Option<f32>,
        mut buffer: TjBuffer,
    ) -> Result<TjBuffer> {
        // Track TJ offset for statistical analysis
        // Per ISO 32000-1:2008 Section 9.4.4, collect all TJ values
        // to detect justified vs normal text through coefficient of variation ~keep
        if self.tj_offset_history.len() < 10000 {
            // Keep history reasonable size (first 10k offsets per document)
            // and update the running accumulators. ~keep
            let x = offset as f64;
            self.tj_sum += x;
            self.tj_sum_sq += x * x;
            self.tj_offset_history.push(offset);
            self.tj_stats_len = self.tj_offset_history.len();
        }

        // Associate TJ offset with the last character
        // The offset applies AFTER the previous string, affecting spacing to next string
        // ~keep
        if !self.tj_character_array.is_empty() {
            let last_idx = self.tj_character_array.len() - 1;
            self.tj_character_array[last_idx].tj_offset = Some(offset as i32);
        }

        // Check if this offset indicates a word boundary
        // Per PDF spec: negative offsets increase spacing
        // Use geometry-based adaptive threshold ~keep
        let threshold = self.calculate_adaptive_tj_threshold();
        // On a tracked array an offset no wider than the array's own letter-
        // spacing is that spacing (or a kerning pair inside it), folded into the
        // advance like any sub-threshold kerning; only a clearly wider offset is
        // a word break. See [`Self::uniform_tj_tracking`]. ~keep
        let is_tracking = tracking.is_some_and(|t| offset >= t - TJ_TRACKING_SPREAD);
        if offset < threshold && !is_tracking {
            self.flush_buffer_at_tj_word_boundary(array, idx, offset, buffer)
        } else {
            // Sub-threshold offset: matrix advances but the
            // current buffer keeps accumulating, so apply
            // the offset unconditionally here as well. ~keep
            self.advance_position_for_offset(offset)?;
            // Fold the same displacement into the buffer's
            // advance record. Historically only the text matrix
            // moved, so these kerning/word-space offsets were
            // dropped from `char_widths`/`accumulated_width` —
            // leaving the span's reconstructed per-glyph positions
            // drifting behind the true render (poppler/PDFium/
            // pymupdf all fold the offset into the advance). On
            // justified body text drawn as one continuous buffer,
            // the many small post-space offsets accumulate into a
            // multi-point undershoot. Folding keeps
            // `sum(char_widths) == accumulated_width == matrix
            // advance` by construction. ~keep
            self.fold_offset_into_buffer(&mut buffer, offset);
            Ok(buffer)
        }
    }

    /// The letter-spacing a `TJ` array applies through its own offsets, if it
    /// draws its text that way: `Some(offset)` when the array places nearly every
    /// glyph on its own, the same offset between each pair, and writes its word
    /// ends as space characters.
    ///
    /// Some producers justify or track a line by putting one string per glyph and
    /// one identical negative offset after each (`[(o)-170 (p)-170 ( )-170 (e)
    /// …] TJ`). Each offset is past the word-boundary threshold, so read one by
    /// one they made every glyph a word: `o p  e e n  a f s t a n d`. The space
    /// characters already mark the words, and an offset the array repeats after
    /// every glyph is spacing, not a boundary. All of:
    /// - at least [`TJ_TRACKING_MIN_SLOTS`] offsets between two strings;
    /// - at least [`TJ_TRACKING_SHORT_STRING_PERCENT`] of the strings one or two
    ///   bytes long (a glyph, or a two-byte code);
    /// - at least [`TJ_TRACKING_COMMON_PERCENT`] of those offsets within
    ///   [`TJ_TRACKING_SPREAD`] of their median, and that median at least
    ///   [`TJ_TRACKING_MIN_OFFSET`] of extra space;
    /// - a space character in one of the strings, and at least
    ///   [`TJ_TRACKING_LETTER_PERCENT`] of the non-space glyphs letters: a row of
    ///   drawing grid labels (`8 7 6 5 4 3 2 1`) is spaced out on purpose.
    ///
    /// An array with no space character has not said where its words end, so it
    /// is tracked only as ONE word (X45): a contract's chapter titles are drawn
    /// one word per array, `[(D)-130(E)-139(F)-132 … (S)] TJ`, and its numbers
    /// the same way, `[(1)-137(0)-133(.)] TJ`; they read `D E F I N I T I O N S`
    /// and `1 0 .`. All of:
    /// - every string one byte, one glyph;
    /// - every offset within [`TJ_TRACKING_SPREAD`] of the median, and that
    ///   median between [`TJ_TRACKING_MIN_OFFSET`] and
    ///   [`TJ_TRACKING_WORD_MAX_OFFSET`]: letter-spacing, not columns;
    /// - the glyphs a word ([`is_tracked_word`]): at least
    ///   [`TJ_TRACKING_WORD_MIN_LETTERS`] letters and nothing else, or a number
    ///   closed by its dot (`10.`). Grid labels (`8 7 6`) are neither.
    pub(super) fn uniform_tj_tracking(&self, array: &[TextElement], font_name: Option<&str>) -> Option<f32> {
        let strings: Vec<&[u8]> = array
            .iter()
            .filter_map(|e| match e {
                TextElement::String(s) => Some(s.as_slice()),
                TextElement::Offset(_) => None,
            })
            .collect();
        if strings.is_empty() {
            return None;
        }
        let mut slots: Vec<f32> = array
            .windows(3)
            .filter_map(|w| match w {
                [TextElement::String(_), TextElement::Offset(o), TextElement::String(_)] => Some(*o),
                _ => None,
            })
            .collect();
        if slots.is_empty() {
            return None;
        }
        slots.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = slots[slots.len() / 2];
        if median > -TJ_TRACKING_MIN_OFFSET {
            return None;
        }
        let common = slots
            .iter()
            .filter(|o| (**o - median).abs() <= TJ_TRACKING_SPREAD)
            .count();
        let font = font_name.and_then(|name| self.fonts.get(name));
        // The same byte -> text order the show-string path decodes with: the
        // font's byte table, then its ToUnicode map, then the extraction
        // fallback. ~keep
        let byte_table = font.map(|f| f.get_byte_to_char_table());
        let decode = |byte: u8| -> Option<char> {
            let Some(f) = font else {
                return byte.is_ascii().then_some(byte as char);
            };
            match byte_table.as_ref().map(|t| t[byte as usize]) {
                Some(c) if c != '\0' => Some(c),
                _ => f
                    .char_to_unicode(byte as u32)
                    .unwrap_or_else(|| fallback_extraction_char_to_unicode(f, byte as u32))
                    .chars()
                    .next(),
            }
        };
        let glyphs: Vec<char> = strings
            .iter()
            .flat_map(|s| s.iter())
            .filter_map(|&b| decode(b))
            .collect();
        if !glyphs.contains(&' ') {
            // X45 -- no space character: the array is tracked only when it is
            // ONE word. ~keep
            let one_glyph_strings = strings.iter().all(|s| s.len() == 1);
            return (one_glyph_strings
                && common == slots.len()
                && median >= -TJ_TRACKING_WORD_MAX_OFFSET
                && is_tracked_word(&glyphs))
            .then_some(median);
        }
        let short = strings.iter().filter(|s| s.len() <= 2).count();
        if short * 100 < strings.len() * TJ_TRACKING_SHORT_STRING_PERCENT {
            return None;
        }
        if slots.len() < TJ_TRACKING_MIN_SLOTS {
            return None;
        }
        if common * 100 < slots.len() * TJ_TRACKING_COMMON_PERCENT {
            return None;
        }
        let letters = glyphs.iter().filter(|c| c.is_alphabetic()).count();
        let inked = glyphs.iter().filter(|c| !c.is_whitespace()).count();
        if letters * 100 < inked * TJ_TRACKING_LETTER_PERCENT {
            return None;
        }
        Some(median)
    }

    /// A `TJ` offset past the word-boundary threshold: flush the current
    /// buffer into a span, insert a space unless one is already implied, and
    /// start a fresh buffer positioned after the offset. Split out of
    /// [`Self::process_tj_offset_element`] to keep it short. ~keep
    fn flush_buffer_at_tj_word_boundary(
        &mut self,
        array: &[TextElement],
        idx: usize,
        offset: f32,
        buffer: TjBuffer,
    ) -> Result<TjBuffer> {
        // Note: split-word symptoms ("diffe rent", "cha nge",
        // "equivalen t") are handled at the higher level by the
        // intra-word kerning guard in `should_insert_space`. An
        // earlier TJ-side guard here (commit b2c6484) used a
        // letter-letter + |offset| < space-glyph-width rule, but
        // that rule misclassified real inter-word gaps in
        // tightly-justified PDFs (LaTeX academic papers, Docling
        // output) where producers encode word boundaries as TJ
        // offsets smaller than a full space glyph. The
        // span-merge-time guard has more context (full bbox,
        // WordBoundaryDetector) and avoids that false positive. ~keep
        //
        // Check if buffer ends with space BEFORE flushing
        // This prevents double spaces when TJ processor inserts space
        // AND span merging would insert space at the same boundary. ~keep
        let buffer_ends_with_space = !buffer.unicode.is_empty()
            && buffer
                .unicode
                .chars()
                .next_back()
                .map(|c| c.is_whitespace())
                .unwrap_or(false);

        self.flush_tj_buffer(buffer)?;

        // Check if the next element in the TJ array is a string
        // that starts with whitespace. If so, DON'T insert a space to avoid doubling.
        // This prevents patterns like "word " + " next" = "word next" (double space)
        // ~keep
        let next_element_starts_with_space = if idx + 1 < array.len() {
            if let TextElement::String(next_s) = &array[idx + 1] {
                next_s
                    .first()
                    .is_some_and(|&byte| byte == 0x20 || byte == 0x09 || byte == 0x0A || byte == 0x0D)
            } else {
                false
            }
        } else {
            false
        };

        if !buffer_ends_with_space && !next_element_starts_with_space {
            self.insert_space_as_span()?;
        }

        // Apply the TJ offset to the text matrix BEFORE
        // creating the new buffer so its `user_pos_x`
        // captures the actual draw position of the next
        // string. Otherwise the buffer anchors at the
        // pre-offset position and every subsequent span
        // on the line inherits the missing tx. ~keep
        self.advance_position_for_offset(offset)?;

        Ok(TjBuffer::new(
            self.state_stack.current(),
            self.current_mcid,
            self.cached_current_font.clone(),
        ))
    }

    /// Process TJ array using primary detection mode.
    ///
    /// This implementation:
    /// 1. Creates BoundaryContext from graphics state
    /// 2. Calls WordBoundaryDetector to detect boundaries in tj_character_array
    /// 3. Apply ligature expansion decisions
    /// 4. Partitions characters into clusters at boundary positions
    /// 5. Converts each cluster to a TextSpan with proper bounding boxes
    /// 6. Marks spans with primary_detected flag
    fn process_tj_array_primary(&mut self, array: &[TextElement]) -> Result<()> {
        if self.tj_character_array.is_empty() {
            return self.process_tj_array_tiebreaker(array);
        }

        // Mark pattern contexts BEFORE boundary detection
        // This protects email and URL patterns from being split at word boundaries ~keep
        let pattern_config = crate::extractors::PatternPreservationConfig::default();
        crate::extractors::PatternDetector::mark_pattern_contexts(&mut self.tj_character_array, &pattern_config)?;

        let context = self.create_boundary_context();

        // Step 3: Create WordBoundaryDetector and detect boundaries
        // OPTIMIZATION: Detect document script profile to skip unnecessary detectors
        // ~keep
        let script = DocumentScript::detect_from_characters(&self.tj_character_array);
        let detector = WordBoundaryDetector::new().with_document_script(script);
        let boundaries = detector.detect_word_boundaries(&self.tj_character_array, &context);

        if boundaries.is_empty() {
            return self.process_tj_array_tiebreaker(array);
        }

        self.apply_ligature_decisions()?;

        let clusters = self.partition_characters_by_boundaries(&self.tj_character_array, boundaries);

        for cluster in clusters {
            if !cluster.is_empty() {
                self.cluster_to_span(&cluster)?;
            }
        }

        Ok(())
    }

    /// Create BoundaryContext from current graphics state.
    ///
    /// Per ISO 32000-1:2008 Section 9.3, extracts text state parameters
    /// used by WordBoundaryDetector to make boundary decisions.
    pub(super) fn create_boundary_context(&self) -> BoundaryContext {
        let state = self.state_stack.current();
        BoundaryContext {
            font_size: state.font_size,
            horizontal_scaling: state.horizontal_scaling,
            word_spacing: state.word_space,
            char_spacing: state.char_space,
        }
    }
}

/// Fewest offsets between two strings a `TJ` array needs before
/// [`TextExtractor::uniform_tj_tracking`] reads a repeated one as letter-spacing.
const TJ_TRACKING_MIN_SLOTS: usize = 6;

/// Share (percent) of an array's strings that must be one or two bytes long --
/// one glyph each -- for [`TextExtractor::uniform_tj_tracking`].
const TJ_TRACKING_SHORT_STRING_PERCENT: usize = 80;

/// Share (percent) of an array's string-to-string offsets that must sit at its
/// median for [`TextExtractor::uniform_tj_tracking`].
const TJ_TRACKING_COMMON_PERCENT: usize = 80;

/// How far (thousandths of an em) an offset may sit from the array's tracking
/// and still count as it in [`TextExtractor::uniform_tj_tracking`], and how much
/// wider than the tracking an offset may be before it is a word break.
const TJ_TRACKING_SPREAD: f32 = 25.0;

/// Smallest tracking (thousandths of an em of extra space) worth reading as
/// such; ordinary kerning sits well below it.
const TJ_TRACKING_MIN_OFFSET: f32 = 50.0;

/// Widest tracking (thousandths of an em of extra space) a spaceless array may
/// carry and still be one tracked word in [`TextExtractor::uniform_tj_tracking`]
/// (X45). The contract that needed it tracks at 0.13 em; past 0.3 em the gaps
/// are wider than a space and the glyphs are no longer read as one word.
const TJ_TRACKING_WORD_MAX_OFFSET: f32 = 300.0;

/// Fewest letters a spaceless tracked array needs to be one word (X45):
/// `RETURN` has six, while two or three spaced capitals can be initials.
const TJ_TRACKING_WORD_MIN_LETTERS: usize = 4;

/// The glyphs of a spaceless tracked array, read as one word (X45): letters
/// only, at least [`TJ_TRACKING_WORD_MIN_LETTERS`] of them; or a section number
/// closed by its dot (`10.`, `3.2.`), which a tracked heading draws in its own
/// array. Digits without the closing dot are labels (a drawing's grid row).
fn is_tracked_word(glyphs: &[char]) -> bool {
    if glyphs.len() >= TJ_TRACKING_WORD_MIN_LETTERS && glyphs.iter().all(|c| c.is_alphabetic()) {
        return true;
    }
    let Some((&'.', number)) = glyphs.split_last() else {
        return false;
    };
    number.first().is_some_and(char::is_ascii_digit)
        && number.last().is_some_and(char::is_ascii_digit)
        && number.iter().all(|c| c.is_ascii_digit() || *c == '.')
        && !number.windows(2).any(|w| w == ['.', '.'])
}

/// Share (percent) of an array's glyphs that must be letters for
/// [`TextExtractor::uniform_tj_tracking`]: tracked text is words, while evenly
/// spaced digits are labels (a drawing's grid row) spaced out on purpose.
const TJ_TRACKING_LETTER_PERCENT: usize = 50;
