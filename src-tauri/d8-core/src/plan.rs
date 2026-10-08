//! Turning a list of files into a shuffled play order.

use std::{collections::BTreeMap, path::PathBuf};

use rand::{seq::SliceRandom, Rng};
use serde::Serialize;

use crate::audio::{needs_transcode, SourceFile, LOSSLESS_EXTS};

#[derive(Debug, Clone)]
pub struct PlannedTrack {
    pub source: PathBuf,
    /// Extension of the source file, lower-case.
    pub ext: String,
    pub size: u64,
    pub transcode: bool,
}

impl PlannedTrack {
    /// Extension the file will have on the card.
    pub fn out_ext(&self) -> &str {
        if self.transcode {
            "mp3"
        } else {
            &self.ext
        }
    }
}

/// Shuffle `files` into a fresh random order. Called once per run with a new
/// RNG, so every run produces a different mix.
pub fn build_plan<R: Rng + ?Sized>(
    mut files: Vec<SourceFile>,
    native: &[String],
    rng: &mut R,
) -> Vec<PlannedTrack> {
    files.shuffle(rng);
    files
        .into_iter()
        .map(|f| PlannedTrack {
            transcode: needs_transcode(&f.ext, native),
            source: f.path,
            ext: f.ext,
            size: f.size,
        })
        .collect()
}

/// How many digits the track numbers need. Three at minimum, so a card that
/// later grows past 99 or 999 songs still sorts the same way.
pub fn number_width(total: usize) -> usize {
    let digits = total.max(1).to_string().len();
    digits.max(3)
}

pub fn track_name(number: usize, width: usize, ext: &str) -> String {
    format!("{number:0width$}.{ext}")
}

/// Rough size of the finished card contents. A V0 MP3 is about a third the
/// size of FLAC; re-encoding a lossy file tends to grow it a little.
pub fn estimate_bytes(files: &[SourceFile], native: &[String]) -> u64 {
    files
        .iter()
        .map(|f| {
            if !needs_transcode(&f.ext, native) {
                f.size
            } else if LOSSLESS_EXTS.contains(&f.ext.as_str()) {
                f.size * 35 / 100
            } else {
                f.size * 11 / 10
            }
        })
        .sum()
}

#[derive(Debug, Clone, Serialize)]
pub struct Preview {
    pub total: usize,
    pub source_bytes: u64,
    pub estimated_bytes: u64,
    pub transcode_count: usize,
    pub by_ext: BTreeMap<String, usize>,
}

pub fn preview(files: &[SourceFile], native: &[String]) -> Preview {
    let mut by_ext = BTreeMap::new();
    for f in files {
        *by_ext.entry(f.ext.clone()).or_insert(0) += 1;
    }
    Preview {
        total: files.len(),
        source_bytes: files.iter().map(|f| f.size).sum(),
        estimated_bytes: estimate_bytes(files, native),
        transcode_count: files
            .iter()
            .filter(|f| needs_transcode(&f.ext, native))
            .count(),
        by_ext,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::{rngs::StdRng, SeedableRng};

    fn files(n: usize) -> Vec<SourceFile> {
        (0..n)
            .map(|i| SourceFile {
                path: PathBuf::from(format!("/music/{i}.mp3")),
                ext: "mp3".into(),
                size: 1000,
            })
            .collect()
    }

    #[test]
    fn plan_contains_every_file_exactly_once() {
        let mut rng = StdRng::seed_from_u64(1);
        let plan = build_plan(files(50), &[], &mut rng);
        let mut seen: Vec<_> = plan.iter().map(|t| t.source.clone()).collect();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 50);
    }

    #[test]
    fn different_runs_give_different_orders() {
        let a = build_plan(files(40), &[], &mut StdRng::seed_from_u64(1));
        let b = build_plan(files(40), &[], &mut StdRng::seed_from_u64(2));
        let order = |p: &[PlannedTrack]| p.iter().map(|t| t.source.clone()).collect::<Vec<_>>();
        assert_ne!(order(&a), order(&b));
    }

    #[test]
    fn numbering_width() {
        assert_eq!(number_width(0), 3);
        assert_eq!(number_width(7), 3);
        assert_eq!(number_width(999), 3);
        assert_eq!(number_width(1000), 4);
        assert_eq!(track_name(7, 3, "mp3"), "007.mp3");
        assert_eq!(track_name(12, 4, "wav"), "0012.wav");
    }

    #[test]
    fn preview_counts_transcodes() {
        let mut f = files(2);
        f.push(SourceFile {
            path: "/m/x.flac".into(),
            ext: "flac".into(),
            size: 1000,
        });
        let p = preview(&f, &[]);
        assert_eq!(p.total, 3);
        assert_eq!(p.transcode_count, 1);
        assert_eq!(p.by_ext["mp3"], 2);
        let keep = preview(&f, &["flac".to_string()]);
        assert_eq!(keep.transcode_count, 0);
        assert_eq!(keep.estimated_bytes, 3000);
    }
}
