//! The run itself: check, wipe, write.
//!
//! Everything that can fail cheaply is checked before the card is touched:
//! the target, the free space, and that ffmpeg exists if anything needs it.
//! Only then is the card wiped.

use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

use rand::thread_rng;
use serde::{Deserialize, Serialize};

use crate::{
    audio::scan_source,
    plan::{build_plan, estimate_bytes, number_width, track_name},
    volumes::df_bytes,
};

#[derive(Debug, Clone, Deserialize)]
pub struct RunOptions {
    pub source: PathBuf,
    pub target: PathBuf,
    /// Extra formats the speaker plays directly, beyond MP3 and WAV.
    #[serde(default)]
    pub native_formats: Vec<String>,
    /// LAME VBR quality, 0 (best) to 9. 0 is roughly 245 kbps.
    #[serde(default)]
    pub mp3_quality: u8,
    pub ffmpeg: Option<PathBuf>,
}

/// Things the caller decides rather than the user, so a front end cannot
/// loosen them.
#[derive(Debug, Clone)]
pub struct Guard {
    /// The target must be a direct child of this folder (/Volumes).
    pub volumes_root: PathBuf,
    pub max_card_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub phase: &'static str,
    pub done: usize,
    pub total: usize,
    pub current: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Failure {
    pub source: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ManifestEntry {
    pub name: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct RunReport {
    pub planned: usize,
    pub written: usize,
    pub transcoded: usize,
    pub copied: usize,
    pub failed: Vec<Failure>,
    pub canceled: bool,
    pub bytes_written: u64,
    pub wiped_items: usize,
    pub manifest: Vec<ManifestEntry>,
    pub elapsed_secs: f64,
}

/* ------------------------------------------------------------------ ffmpeg -- */

/// A GUI app on macOS does not inherit the shell PATH, so Homebrew's folders
/// are checked by hand.
pub fn find_ffmpeg() -> Option<PathBuf> {
    let fixed = [
        "/opt/homebrew/bin/ffmpeg",
        "/usr/local/bin/ffmpeg",
        "/opt/local/bin/ffmpeg",
        "/usr/bin/ffmpeg",
    ];
    for p in fixed {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Some(p);
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("ffmpeg"))
        .find(|p| p.is_file())
}

/// `Ok` only if this ffmpeg can actually write MP3.
pub fn check_ffmpeg(ffmpeg: &Path) -> Result<(), String> {
    let out = Command::new(ffmpeg)
        .args(["-hide_banner", "-encoders"])
        .output()
        .map_err(|e| format!("ffmpeg at {} would not start: {e}", ffmpeg.display()))?;
    if String::from_utf8_lossy(&out.stdout).contains("libmp3lame") {
        Ok(())
    } else {
        Err("This ffmpeg has no MP3 encoder (libmp3lame). Install one with: brew install ffmpeg".into())
    }
}

fn transcode(ffmpeg: &Path, src: &Path, dst: &Path, quality: u8) -> Result<(), String> {
    let out = Command::new(ffmpeg)
        .args(["-nostdin", "-y", "-v", "error", "-i"])
        .arg(src)
        .args([
            "-vn", // no cover art: it only bloats the file
            "-map_metadata",
            "0",
            "-ac",
            "2",
            "-codec:a",
            "libmp3lame",
            "-q:a",
        ])
        .arg(quality.min(9).to_string())
        .arg(dst)
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/* ------------------------------------------------------------------- guards -- */

/// Names macOS and Windows put on a card themselves. A failure to remove one
/// is not worth stopping for.
fn is_system_name(name: &str) -> bool {
    name.starts_with("._")
        || matches!(
            name,
            ".Spotlight-V100"
                | ".fseventsd"
                | ".Trashes"
                | ".TemporaryItems"
                | ".DocumentRevisions-V100"
                | ".DS_Store"
                | "System Volume Information"
        )
}

fn check_target(target: &Path, source: &Path, guard: &Guard) -> Result<(), String> {
    let meta = fs::symlink_metadata(target).map_err(|e| format!("Card not found: {e}"))?;
    if meta.file_type().is_symlink() {
        return Err("The target is a shortcut, not a card. Refusing.".into());
    }
    if !meta.is_dir() {
        return Err("The target is not a folder.".into());
    }

    let canon_target = fs::canonicalize(target).map_err(|e| e.to_string())?;
    let canon_root = fs::canonicalize(&guard.volumes_root).map_err(|e| e.to_string())?;
    if canon_target.parent() != Some(canon_root.as_path()) {
        return Err(format!(
            "The target must be a mounted volume directly under {}.",
            guard.volumes_root.display()
        ));
    }

    let canon_source = fs::canonicalize(source).map_err(|e| format!("Source not found: {e}"))?;
    if canon_source.starts_with(&canon_target) || canon_target.starts_with(&canon_source) {
        return Err("The music folder and the card overlap. Refusing to wipe the source.".into());
    }
    Ok(())
}

/// What is on a card right now, shown before the user agrees to wipe it.
#[derive(Debug, Clone, Serialize)]
pub struct CardContents {
    pub items: usize,
    pub bytes: u64,
}

pub fn inspect_card(target: &Path) -> Result<CardContents, String> {
    let mut items = 0usize;
    let mut bytes = 0u64;
    let mut stack = vec![target.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let rd = fs::read_dir(&dir).map_err(|e| e.to_string())?;
        for entry in rd.flatten() {
            let name = entry.file_name();
            if dir == target && is_system_name(&name.to_string_lossy()) {
                continue;
            }
            let Ok(ft) = entry.file_type() else { continue };
            if ft.is_dir() {
                stack.push(entry.path());
            } else {
                items += 1;
                bytes += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
    }
    Ok(CardContents { items, bytes })
}

fn wipe_card(target: &Path) -> Result<usize, String> {
    let mut removed = 0usize;
    let mut stuck = Vec::new();
    let rd = fs::read_dir(target).map_err(|e| format!("Could not read the card: {e}"))?;
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(ft) = entry.file_type() else { continue };
        let result = if ft.is_dir() {
            fs::remove_dir_all(entry.path())
        } else {
            fs::remove_file(entry.path())
        };
        match result {
            Ok(()) => removed += 1,
            Err(_) if is_system_name(&name) => {}
            Err(e) => stuck.push(format!("{name}: {e}")),
        }
    }
    if stuck.is_empty() {
        Ok(removed)
    } else {
        Err(format!(
            "Could not clear the card (is it write-protected?): {}",
            stuck.join("; ")
        ))
    }
}

/// Drop the junk macOS scatters on a FAT card: `._name` companions would show
/// up on the speaker as extra tracks.
fn remove_mac_litter(target: &Path) {
    if let Ok(rd) = fs::read_dir(target) {
        for entry in rd.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with("._") || name == ".DS_Store" {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
}

/* ---------------------------------------------------------------------- run -- */

pub fn run(
    opts: &RunOptions,
    guard: &Guard,
    cancel: &AtomicBool,
    progress: &mut dyn FnMut(Progress),
) -> Result<RunReport, String> {
    let started = Instant::now();
    let mut report = RunReport::default();

    progress(Progress { phase: "checking", done: 0, total: 0, current: "Reading your music folder".into() });
    check_target(&opts.target, &opts.source, guard)?;

    let files = scan_source(&opts.source).map_err(|e| format!("Could not read the music folder: {e}"))?;
    if files.is_empty() {
        return Err("No audio files found in the music folder.".into());
    }

    let (total_bytes, _free) = df_bytes(&opts.target)?;
    if total_bytes > guard.max_card_bytes {
        return Err("That volume is far larger than a memory card. Refusing to wipe it.".into());
    }
    let needed = estimate_bytes(&files, &opts.native_formats);
    // FAT32 keeps a few percent for itself.
    if needed > total_bytes / 100 * 97 {
        return Err(format!(
            "The music will not fit: about {:.1} GB of tracks for a {:.1} GB card.",
            needed as f64 / 1e9,
            total_bytes as f64 / 1e9
        ));
    }

    let plan = build_plan(files, &opts.native_formats, &mut thread_rng());
    report.planned = plan.len();

    let ffmpeg = if plan.iter().any(|t| t.transcode) {
        let path = opts
            .ffmpeg
            .clone()
            .or_else(find_ffmpeg)
            .ok_or("ffmpeg is needed to convert some files but was not found. Install it with: brew install ffmpeg")?;
        check_ffmpeg(&path)?;
        Some(path)
    } else {
        None
    };

    // Past this line the card is changed.
    progress(Progress { phase: "wiping", done: 0, total: plan.len(), current: "Clearing the card".into() });
    report.wiped_items = wipe_card(&opts.target)?;
    // Tells Spotlight not to index the card, so it does not litter it.
    let _ = fs::write(opts.target.join(".metadata_never_index"), b"");

    let width = number_width(plan.len());
    let mut number = 0usize;

    for (i, track) in plan.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            report.canceled = true;
            break;
        }
        let shown = track
            .source
            .file_name()
            .unwrap_or_else(|| OsStr::new("?"))
            .to_string_lossy()
            .into_owned();
        progress(Progress { phase: "copying", done: i, total: plan.len(), current: shown });

        // Numbered by what actually lands on the card, so a file that fails to
        // convert leaves no hole in the sequence.
        let name = track_name(number + 1, width, track.out_ext());
        let dst = opts.target.join(&name);

        if track.transcode {
            let ffmpeg = ffmpeg.as_deref().expect("ffmpeg checked above");
            if let Err(e) = transcode(ffmpeg, &track.source, &dst, opts.mp3_quality) {
                let _ = fs::remove_file(&dst);
                report.failed.push(Failure { source: track.source.display().to_string(), error: e });
                continue;
            }
            report.transcoded += 1;
        } else {
            fs::copy(&track.source, &dst).map_err(|e| {
                format!("Copying {} failed: {e}. The card may be full or removed.", track.source.display())
            })?;
            report.copied += 1;
        }

        number += 1;
        report.written += 1;
        report.bytes_written += fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
        report.manifest.push(ManifestEntry { name, source: track.source.display().to_string() });
    }

    progress(Progress { phase: "finishing", done: report.written, total: plan.len(), current: "Tidying up".into() });
    remove_mac_litter(&opts.target);

    report.elapsed_secs = started.elapsed().as_secs_f64();
    progress(Progress { phase: "done", done: report.written, total: plan.len(), current: String::new() });
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeSet, fs::File, io::Write};

    struct Rig {
        _dir: tempfile::TempDir,
        volumes: PathBuf,
        card: PathBuf,
        source: PathBuf,
    }

    fn rig() -> Rig {
        let dir = tempfile::tempdir().unwrap();
        let volumes = dir.path().join("Volumes");
        let card = volumes.join("MUSIC");
        let source = dir.path().join("D8");
        fs::create_dir_all(&card).unwrap();
        fs::create_dir_all(source.join("Rock/Live")).unwrap();
        fs::create_dir_all(source.join("Pop")).unwrap();
        for (i, rel) in [
            "one.mp3",
            "Rock/two.mp3",
            "Rock/Live/three.wav",
            "Pop/four.mp3",
            "Pop/five.mp3",
            "six.mp3",
        ]
        .iter()
        .enumerate()
        {
            let mut f = File::create(source.join(rel)).unwrap();
            f.write_all(&vec![i as u8; 2048 + i]).unwrap();
        }
        File::create(source.join("Pop/cover.jpg")).unwrap();
        Rig { _dir: dir, volumes, card, source }
    }

    fn guard(r: &Rig) -> Guard {
        Guard { volumes_root: r.volumes.clone(), max_card_bytes: u64::MAX }
    }

    fn opts(r: &Rig) -> RunOptions {
        RunOptions {
            source: r.source.clone(),
            target: r.card.clone(),
            native_formats: vec![],
            mp3_quality: 0,
            ffmpeg: None,
        }
    }

    fn tree(p: &Path) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut stack = vec![p.to_path_buf()];
        while let Some(d) = stack.pop() {
            for e in fs::read_dir(&d).unwrap().flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let len = fs::metadata(&path).unwrap().len();
                    out.insert(format!("{}:{len}", path.strip_prefix(p).unwrap().display()));
                }
            }
        }
        out
    }

    #[test]
    fn wipes_flattens_numbers_and_leaves_the_source_alone() {
        let r = rig();
        // Junk already on the card.
        fs::create_dir_all(r.card.join("old/albums")).unwrap();
        File::create(r.card.join("old/albums/x.mp3")).unwrap();
        File::create(r.card.join("stale.mp3")).unwrap();
        File::create(r.card.join("._stale.mp3")).unwrap();

        let before = tree(&r.source);
        let cancel = AtomicBool::new(false);
        let mut phases = Vec::new();
        let report = run(&opts(&r), &guard(&r), &cancel, &mut |p| phases.push(p.phase)).unwrap();

        assert_eq!(report.written, 5 + 1);
        assert_eq!(report.copied, 6);
        assert_eq!(report.transcoded, 0);
        assert!(report.failed.is_empty());
        assert!(report.wiped_items >= 3);
        assert_eq!(phases.first(), Some(&"checking"));
        assert_eq!(phases.last(), Some(&"done"));

        // The card is flat, numbered 001..006, with nothing else audible.
        let mut names: Vec<String> = fs::read_dir(&r.card)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !n.starts_with('.'))
            .collect();
        names.sort();
        let stems: Vec<_> = names.iter().map(|n| n.split('.').next().unwrap().to_string()).collect();
        assert_eq!(stems, ["001", "002", "003", "004", "005", "006"]);
        assert!(r.card.read_dir().unwrap().flatten().all(|e| e.path().is_file()));
        assert!(!r.card.join("old").exists());
        assert!(!r.card.join("stale.mp3").exists());
        assert!(!r.card.join("._stale.mp3").exists());

        // Source is byte-for-byte what it was.
        assert_eq!(tree(&r.source), before);
        // Manifest maps every number to a real source file.
        assert_eq!(report.manifest.len(), 6);
        assert!(report.manifest.iter().all(|m| Path::new(&m.source).exists()));
    }

    #[test]
    fn two_runs_give_two_orders() {
        let r = rig();
        let cancel = AtomicBool::new(false);
        let order = |rep: &RunReport| rep.manifest.iter().map(|m| m.source.clone()).collect::<Vec<_>>();
        let mut seen = BTreeSet::new();
        for _ in 0..8 {
            let rep = run(&opts(&r), &guard(&r), &cancel, &mut |_| {}).unwrap();
            seen.insert(order(&rep));
        }
        assert!(seen.len() > 1, "eight runs all produced the same order");
    }

    #[test]
    fn refuses_a_target_that_is_not_a_direct_child_of_volumes() {
        let r = rig();
        let deep = r.card.join("sub");
        fs::create_dir_all(&deep).unwrap();
        let mut o = opts(&r);
        o.target = deep;
        let cancel = AtomicBool::new(false);
        let err = run(&o, &guard(&r), &cancel, &mut |_| {}).unwrap_err();
        assert!(err.contains("mounted volume"), "{err}");
        // And nothing was touched.
        assert!(r.card.join("sub").exists());
    }

    #[test]
    fn refuses_to_wipe_the_source() {
        let r = rig();
        let mut o = opts(&r);
        // Pretend the music lives on the card.
        o.source = r.card.clone();
        let cancel = AtomicBool::new(false);
        let err = run(&o, &guard(&r), &cancel, &mut |_| {}).unwrap_err();
        assert!(err.contains("overlap"), "{err}");
    }

    #[test]
    fn refuses_a_symlinked_target() {
        let r = rig();
        let link = r.volumes.join("LINK");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&r.source, &link).unwrap();
        let mut o = opts(&r);
        o.target = link;
        let cancel = AtomicBool::new(false);
        assert!(run(&o, &guard(&r), &cancel, &mut |_| {}).is_err());
        assert!(r.source.join("one.mp3").exists());
    }

    #[test]
    fn refuses_an_oversized_volume() {
        let r = rig();
        let g = Guard { volumes_root: r.volumes.clone(), max_card_bytes: 1024 };
        let cancel = AtomicBool::new(false);
        let err = run(&opts(&r), &g, &cancel, &mut |_| {}).unwrap_err();
        assert!(err.contains("larger than a memory card"), "{err}");
    }

    #[test]
    fn empty_source_is_an_error_and_leaves_the_card_alone() {
        let r = rig();
        let empty = r.source.parent().unwrap().join("empty");
        fs::create_dir_all(&empty).unwrap();
        File::create(r.card.join("keep.mp3")).unwrap();
        let mut o = opts(&r);
        o.source = empty;
        let cancel = AtomicBool::new(false);
        assert!(run(&o, &guard(&r), &cancel, &mut |_| {}).is_err());
        assert!(r.card.join("keep.mp3").exists());
    }

    #[test]
    fn a_missing_ffmpeg_fails_before_the_wipe() {
        let r = rig();
        File::create(r.source.join("track.flac")).unwrap();
        File::create(r.card.join("keep.mp3")).unwrap();
        let mut o = opts(&r);
        o.ffmpeg = Some(PathBuf::from("/definitely/not/ffmpeg"));
        let cancel = AtomicBool::new(false);
        assert!(run(&o, &guard(&r), &cancel, &mut |_| {}).is_err());
        assert!(r.card.join("keep.mp3").exists(), "card was wiped before the ffmpeg check");
    }

    #[test]
    fn flac_is_transcoded_to_mp3_when_ffmpeg_is_present() {
        let Some(ffmpeg) = find_ffmpeg() else {
            eprintln!("skipping: no ffmpeg on this machine");
            return;
        };
        if check_ffmpeg(&ffmpeg).is_err() {
            eprintln!("skipping: ffmpeg has no libmp3lame");
            return;
        }
        let r = rig();
        let flac = r.source.join("Rock/tone.flac");
        let made = Command::new(&ffmpeg)
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:duration=1"])
            .arg(&flac)
            .status()
            .unwrap();
        assert!(made.success());
        // A deliberately broken one: should be reported, not abort the run.
        fs::write(r.source.join("broken.flac"), b"not audio at all").unwrap();

        let cancel = AtomicBool::new(false);
        let rep = run(&opts(&r), &guard(&r), &cancel, &mut |_| {}).unwrap();
        assert_eq!(rep.transcoded, 1);
        assert_eq!(rep.failed.len(), 1);
        assert!(rep.failed[0].source.ends_with("broken.flac"));
        // 6 plain + 1 converted, numbered with no gap despite the failure.
        assert_eq!(rep.written, 7);
        let mut nums: Vec<String> = rep.manifest.iter().map(|m| m.name.clone()).collect();
        nums.sort();
        assert_eq!(nums.first().unwrap().split('.').next().unwrap(), "001");
        assert_eq!(nums.last().unwrap().split('.').next().unwrap(), "007");
        let converted = rep.manifest.iter().find(|m| m.source.ends_with("tone.flac")).unwrap();
        assert!(converted.name.ends_with(".mp3"));
        let bytes = fs::read(r.card.join(&converted.name)).unwrap();
        assert!(bytes.starts_with(b"ID3") || (bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0));
        assert!(!r.card.join("008.mp3").exists());
    }

    #[test]
    fn cancel_stops_after_the_wipe_and_says_so() {
        let r = rig();
        let cancel = AtomicBool::new(true);
        let rep = run(&opts(&r), &guard(&r), &cancel, &mut |_| {}).unwrap();
        assert!(rep.canceled);
        assert_eq!(rep.written, 0);
    }

    #[test]
    fn inspect_counts_the_card() {
        let r = rig();
        fs::create_dir_all(r.card.join("a")).unwrap();
        fs::write(r.card.join("a/x.mp3"), vec![0u8; 100]).unwrap();
        fs::write(r.card.join("y.mp3"), vec![0u8; 50]).unwrap();
        fs::write(r.card.join("._y.mp3"), vec![0u8; 5]).unwrap();
        let c = inspect_card(&r.card).unwrap();
        assert_eq!(c.items, 2);
        assert_eq!(c.bytes, 150);
    }
}
