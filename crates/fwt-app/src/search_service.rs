use std::{ops::Range, sync::Arc};

use fwt_domain::{
    ports::{CatalogRepository, RepositoryError},
    widget::{SearchCorpusEntry, WidgetId},
};
use nucleo_matcher::{
    pattern::{CaseMatching, Normalization, Pattern},
    Config, Matcher, Utf32Str,
};

/// Coarse FTS5 pre-filter size fed into fine-ranking. Generous relative to
/// the MVP ~100-widget corpus; revisit against real benchmark data as the
/// catalog grows toward NFR-3's 350–500 widget target.
// TODO(post-Epic-2, revisit against full-corpus benchmark)
const COARSE_LIMIT: usize = 200;

/// Multi-field weighting (TRD Section 6 step 4): name > category > summary.
/// A documented table, not scattered magic numbers.
const WEIGHT_NAME: f64 = 1.0;
const WEIGHT_CATEGORY: f64 = 0.6;
const WEIGHT_SUMMARY: f64 = 0.35;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub widget_id: WidgetId,
    pub name: String,
    pub summary: String,
    pub score: f64,
    pub name_highlight_ranges: Vec<Range<usize>>,
}

/// In-memory index entry — built once from `load_search_corpus()`.
pub struct IndexEntry {
    id: WidgetId,
    name: String,
    categories: Vec<String>,
    summary: String,
}

pub struct SearchService {
    repository: Arc<dyn CatalogRepository>,
    index: Option<Vec<IndexEntry>>,
    matcher: Matcher,
}
impl SearchService {
    pub fn new(repository: Arc<dyn CatalogRepository>) -> Self {
        Self {
            repository,
            index: None,
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
        }
    }

    pub fn is_index_ready(&self) -> bool {
        self.index.is_some()
    }

    pub async fn build_index(&mut self) -> Result<(), RepositoryError> {
        let corpus: Vec<SearchCorpusEntry> = self.repository.load_search_corpus().await?;
        self.index = Some(
            corpus
                .into_iter()
                .map(|c| IndexEntry {
                    id: c.id,
                    name: c.name,
                    categories: c.categories,
                    summary: c.summary,
                })
                .collect(),
        );
        Ok(())
    }

    pub async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, RepositoryError> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let Some(index) = &self.index else {
            return Ok(Vec::new());
        };

        // Stage 1: coarse FTS5 filter narrows the corpus.
        let coarse = self.repository.search_fts(query, COARSE_LIMIT).await?;
        if coarse.is_empty() {
            return Ok(Vec::new());
        }
        let coarse_ids: std::collections::HashSet<WidgetId> = coarse.iter().map(|w| w.id).collect();

        // Stage 2: fine-rank ONLY the coarse candidate set, never the
        // full corpus — this is the property the dedicated test in
        // Phase 5 asserts against a mock repository.
        let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
        let mut matcher = self.matcher.clone();

        let mut scored: Vec<SearchResult> = Vec::new();

        for entry in index.iter().filter(|e| coarse_ids.contains(&e.id)) {
            let mut buf = Vec::new();
            let mut indices = Vec::new();

            let name_haystack = Utf32Str::new(&entry.name, &mut buf);
            let name_score = pattern
                .indices(name_haystack, &mut matcher, &mut indices)
                .unwrap_or(0);

            let category_text = entry.categories.join(" ");
            let mut cat_buf = Vec::new();
            let category_score = pattern
                .score(Utf32Str::new(&category_text, &mut cat_buf), &mut matcher)
                .unwrap_or(0);

            let mut sum_buf = Vec::new();
            let summary_score = pattern
                .score(Utf32Str::new(&entry.summary, &mut sum_buf), &mut matcher)
                .unwrap_or(0);

            if name_score == 0 && category_score == 0 && summary_score == 0 {
                continue;
            }

            let weighted = (name_score as f64 * WEIGHT_NAME)
                + (category_score as f64 * WEIGHT_CATEGORY)
                + (summary_score as f64 * WEIGHT_SUMMARY);

            let ranges = char_indices_to_byte_ranges(&entry.name, &indices);

            scored.push(SearchResult {
                widget_id: entry.id,
                name: entry.name.clone(),
                summary: entry.summary.clone(),
                score: weighted,
                name_highlight_ranges: ranges,
            });
        }

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        Ok(scored)
    }
}

/// nucleo's `Pattern::indices` gives CHAR indices into the haystack, not
/// byte offsets — Ratatui spans need byte ranges. Converts and collapses
/// adjacent/overlapping char indices into contiguous byte ranges.
fn char_indices_to_byte_ranges(text: &str, char_indices: &[u32]) -> Vec<Range<usize>> {
    if char_indices.is_empty() {
        return Vec::new();
    }
    let mut sorted: Vec<u32> = char_indices.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    // char index -> byte offset lookup table (one pass over the string).
    let byte_offsets: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    let char_len = |i: usize| {
        text[byte_offsets[i]..]
            .chars()
            .next()
            .map_or(1, |c| c.len_utf8())
    };

    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut run_start_char = sorted[0] as usize;
    let mut run_end_char = sorted[0] as usize;

    for &c in &sorted[1..] {
        let c = c as usize;
        if c == run_end_char + 1 {
            run_end_char = c;
        } else {
            let start = byte_offsets[run_start_char];
            let end = byte_offsets[run_end_char] + char_len(run_end_char);
            ranges.push(start..end);
            run_start_char = c;
            run_end_char = c;
        }
    }
    let start = byte_offsets[run_start_char];
    let end = byte_offsets[run_end_char] + char_len(run_end_char);
    ranges.push(start..end);
    ranges
}
