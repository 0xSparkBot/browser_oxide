//! Shaping, with the cache granularity PoC-1 measured rather than the one that
//! looks natural.
//!
//! The obvious key is the itemised run: `(face, size, script, direction,
//! text)`. Measured, that gives 17–75× on relayout and **0–1 cache hits out of
//! 1–19 lookups on first paint** — because an itemised run is usually the whole
//! paragraph, and no two paragraphs are identical. The cache was cold exactly
//! when it mattered.
//!
//! Blink caches per *word* for this reason, and so do we, for runs where
//! splitting on spaces cannot change the result. It cannot for simple LTR text
//! with no complex shaping; it very much can for Arabic (joining forms cross
//! the boundary) and for anything with kerning pairs involving a space, so
//! those fall back to whole-run keys. On PoC-1's corpus the word-granular key
//! exposes 31% reuse on English prose and 0% on German compounds and URLs —
//! which is why the fallback is not optional and why the cache is keyed, not
//! assumed.

use std::collections::HashMap;
use std::sync::Arc;

use rustybuzz::{Direction, Face as HbFace, UnicodeBuffer};

/// One positioned glyph.
#[derive(Debug, Clone, Copy)]
pub struct Glyph {
    pub id: u16,
    /// Byte offset of this glyph's cluster within the shaped text.
    pub cluster: u32,
    pub x_advance: f32,
    pub x_offset: f32,
    pub y_offset: f32,
}

#[derive(PartialEq, Eq, Hash)]
struct Key {
    face: u64,
    size_bits: u32,
    script: u32,
    rtl: bool,
    text_hash: u64,
    text_len: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
}

impl CacheStats {
    pub fn lookups(&self) -> u64 {
        self.hits + self.misses
    }
}

/// A shaping cache. Not `Sync`: layout is single-threaded and a lock here would
/// cost more than the cache saves.
#[derive(Default)]
pub struct ShapeCache {
    map: HashMap<Key, Arc<Vec<Glyph>>>,
    pub stats: CacheStats,
}

impl ShapeCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.map.clear();
        self.stats = CacheStats::default();
    }

    /// Shape `text` and return glyphs whose clusters are relative to it.
    ///
    /// `face_key` identifies the face for caching; it must be stable for the
    /// lifetime of the cache and distinct per face.
    pub fn shape(
        &mut self,
        face: &HbFace<'_>,
        face_key: u64,
        size_px: f32,
        script: rustybuzz::Script,
        rtl: bool,
        text: &str,
    ) -> Arc<Vec<Glyph>> {
        let key = Key {
            face: face_key,
            size_bits: size_px.to_bits(),
            script: script.tag().as_u32(),
            rtl,
            text_hash: fnv1a(text.as_bytes()),
            text_len: text.len(),
        };
        if let Some(hit) = self.map.get(&key) {
            self.stats.hits += 1;
            return Arc::clone(hit);
        }
        self.stats.misses += 1;
        let glyphs = Arc::new(shape_uncached(face, size_px, script, rtl, text));
        self.map.insert(key, Arc::clone(&glyphs));
        glyphs
    }

    /// Total advance of `text`, using per-word cache entries when that cannot
    /// change the result.
    ///
    /// Word splitting is only safe when the text is simple: LTR, no marks, no
    /// joining scripts. `can_split_words` decides; when it says no, this is a
    /// single whole-run shape and the cache behaves exactly as before.
    pub fn measure(
        &mut self,
        face: &HbFace<'_>,
        face_key: u64,
        size_px: f32,
        script: rustybuzz::Script,
        rtl: bool,
        text: &str,
    ) -> f32 {
        if !can_split_words(text, rtl) {
            return self
                .shape(face, face_key, size_px, script, rtl, text)
                .iter()
                .map(|g| g.x_advance)
                .sum();
        }

        let mut total = 0.0;
        for piece in split_keeping_spaces(text) {
            total += self
                .shape(face, face_key, size_px, script, rtl, piece)
                .iter()
                .map(|g| g.x_advance)
                .sum::<f32>();
        }
        total
    }
}

/// Is it safe to shape this text one word at a time?
///
/// Splitting changes the result whenever shaping crosses a space: Arabic and
/// other cursive scripts join across one, marks attach across one, and a font
/// may kern a pair that includes it. The check is deliberately conservative —
/// a false "no" costs a cache miss, a false "yes" costs wrong glyph advances,
/// and wrong advances mean wrong line breaks.
fn can_split_words(text: &str, rtl: bool) -> bool {
    if rtl {
        return false;
    }
    text.chars().all(|c| {
        // Latin, Greek, Cyrillic and common punctuation are safe. Anything
        // above the Cyrillic block may join, combine or reorder.
        c < '\u{0590}' && !matches!(c, '\u{0300}'..='\u{036F}')
    })
}

/// Split into words, keeping each trailing space attached to the word before
/// it so the advances still sum to the whole run's width.
fn split_keeping_spaces(text: &str) -> impl Iterator<Item = &str> {
    let mut rest = text;
    std::iter::from_fn(move || {
        if rest.is_empty() {
            return None;
        }
        // Take up to and including the next run of spaces.
        let after_word = rest.find(' ').map_or(rest.len(), |i| i);
        let mut end = after_word;
        while rest[end..].starts_with(' ') {
            end += 1;
        }
        if end == 0 {
            end = rest.len();
        }
        let (piece, tail) = rest.split_at(end);
        rest = tail;
        Some(piece)
    })
}

fn shape_uncached(
    face: &HbFace<'_>,
    size_px: f32,
    script: rustybuzz::Script,
    rtl: bool,
    text: &str,
) -> Vec<Glyph> {
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.set_direction(if rtl {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    });
    buffer.set_script(script);

    let output = rustybuzz::shape(face, &[], buffer);
    let upem = (face.units_per_em() as f32).max(1.0);
    let scale = size_px / upem;

    output
        .glyph_infos()
        .iter()
        .zip(output.glyph_positions().iter())
        .map(|(info, pos)| Glyph {
            id: info.glyph_id as u16,
            cluster: info.cluster,
            x_advance: pos.x_advance as f32 * scale,
            x_offset: pos.x_offset as f32 * scale,
            y_offset: pos.y_offset as f32 * scale,
        })
        .collect()
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    const SANS: &[u8] = include_bytes!("../../canvas/fonts/LiberationSans-Regular.ttf");

    fn face() -> HbFace<'static> {
        HbFace::from_slice(SANS, 0).expect("bundled face parses")
    }

    #[test]
    fn word_splitting_matches_whole_run_measurement() {
        // The load-bearing property: the word-granular fast path must produce
        // exactly the same width as shaping the run in one go, or the cache
        // silently changes where lines break.
        let f = face();
        let text = "the quick brown fox jumps over the lazy dog";

        let mut whole = ShapeCache::new();
        let whole_width: f32 = whole
            .shape(&f, 1, 16.0, rustybuzz::script::LATIN, false, text)
            .iter()
            .map(|g| g.x_advance)
            .sum();

        let mut split = ShapeCache::new();
        let split_width = split.measure(&f, 1, 16.0, rustybuzz::script::LATIN, false, text);

        assert!(
            (whole_width - split_width).abs() < 0.01,
            "word-split width {split_width} != whole-run width {whole_width}"
        );
    }

    #[test]
    fn repeated_words_hit_the_cache() {
        // The whole point of word granularity: "the" appears twice, so the
        // second occurrence must not reshape.
        let f = face();
        let mut cache = ShapeCache::new();
        cache.measure(
            &f,
            1,
            16.0,
            rustybuzz::script::LATIN,
            false,
            "the quick the lazy the end",
        );
        assert!(
            cache.stats.hits > 0,
            "expected reuse across repeated words, got {:?}",
            cache.stats
        );
    }

    #[test]
    fn rtl_text_is_never_word_split() {
        // Arabic joins across spaces; splitting would produce isolated forms
        // and wrong advances.
        assert!(!can_split_words("العربية لغة", true));
        assert!(!can_split_words("العربية لغة", false));
    }

    #[test]
    fn combining_marks_are_never_word_split() {
        assert!(!can_split_words("e\u{0301}", false));
    }

    #[test]
    fn split_pieces_reconstitute_the_input() {
        let text = "a bb  ccc ";
        let joined: String = split_keeping_spaces(text).collect();
        assert_eq!(joined, text);
    }

    #[test]
    fn empty_text_shapes_to_nothing() {
        let f = face();
        let mut cache = ShapeCache::new();
        assert_eq!(
            cache.measure(&f, 1, 16.0, rustybuzz::script::LATIN, false, ""),
            0.0
        );
    }
}
