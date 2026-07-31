// #[derive(Clone)]
// pub struct SpikeCandidate {
//     pub name: String,
//     pub categories: Vec<String>,
//     pub summary: String,
// }

// pub struct SpikeMatch {
//     pub index: usize,
//     pub score: i64,
//     /// byte-offset ranges into `name` — used to judge highlighting ergonomics
//     pub highlight_ranges: Option<Vec<std::ops::Range<usize>>>,
// }

// pub trait SpikeMatcher {
//     fn name(&self) -> &'static str;
//     fn build(&mut self, corpus: &[SpikeCandidate]);
//     fn query(&self, query: &str) -> Vec<SpikeMatch>;
// }

// pub struct NucleoSpike {
//     corpus: Vec<SpikeCandidate>,
//     matcher: nucleo_matcher::Matcher,
// }

// impl SpikeMatcher for NucleoSpike {
//     fn name(&self) -> &'static str {
//         "nucleo"
//     }
//     fn build(&mut self, corpus: &[SpikeCandidate]) {
//         self.corpus = corpus.to_vec();
//     }
//     fn query(&self, query: &str) -> Vec<SpikeMatch> {
//         use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
//         let pattern = Pattern::parse(query, CaseMatching::Ignore, Normalization::Smart);
//         let mut matcher = self.matcher.clone();
//         let mut out = Vec::new();
//         for (i, c) in self.corpus.iter().enumerate() {
//             let mut buf = Vec::new();
//             let haystack = nucleo_matcher::Utf32Str::new(&c.name, &mut buf);
//             if let Some(score) = pattern.score(haystack, &mut matcher) {
//                 out.push(SpikeMatch {
//                     index: i,
//                     score: score as i64,
//                     highlight_ranges: None,
//                 });
//             }
//         }
//         out.sort_by(|a, b| b.score.cmp(&a.score));
//         out
//     }
// }
