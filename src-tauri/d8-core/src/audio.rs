//! Finding the music, and deciding what the speaker can play as-is.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use serde::Serialize;

/// Everything the tool will pick up. Anything else in the folder is ignored.
pub const AUDIO_EXTS: &[&str] = &[
    "mp3", "wav", "flac", "m4a", "aac", "ogg", "oga", "opus", "wma", "ape", "wv", "aif", "aiff",
];

/// Formats that are large enough to shrink a lot when turned into MP3. Used
/// only to estimate how much space the finished card will take.
pub const LOSSLESS_EXTS: &[&str] = &["flac", "wav", "ape", "wv", "aif", "aiff"];

#[derive(Debug, Clone, Serialize)]
pub struct SourceFile {
    pub path: PathBuf,
    /// Lower-cased extension, no dot.
    pub ext: String,
    pub size: u64,
}

pub fn ext_of(path: &Path) -> Option<String> {
    path.extension()?.to_str().map(|s| s.to_ascii_lowercase())
}

/// Recursively collect every audio file under `root`.
///
/// Hidden files and folders (anything starting with a dot) are skipped, which
/// also keeps macOS `._*` resource-fork files out of the shuffle. Symlinked
/// folders are not followed, so a loop cannot hang the scan; symlinked files
/// are.
pub fn scan_source(root: &Path) -> io::Result<Vec<SourceFile>> {
    let meta = fs::metadata(root)?;
    if !meta.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the source is not a folder",
        ));
    }

    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(rd) => rd,
            // An unreadable subfolder should not sink the whole scan.
            Err(_) if dir != root => continue,
            Err(e) => return Err(e),
        };

        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            if file_type.is_dir() {
                stack.push(path);
                continue;
            }

            let md = if file_type.is_symlink() {
                fs::metadata(&path)
            } else {
                entry.metadata()
            };
            let Ok(md) = md else { continue };
            if !md.is_file() {
                continue;
            }

            if let Some(ext) = ext_of(&path) {
                if AUDIO_EXTS.contains(&ext.as_str()) {
                    out.push(SourceFile {
                        path,
                        ext,
                        size: md.len(),
                    });
                }
            }
        }
    }

    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// MP3 and WAV are assumed to play on any speaker with a card slot. Anything
/// else has to be listed in `native` (the user tells us, for example, that
/// FLAC works) or it gets transcoded.
pub fn needs_transcode(ext: &str, native: &[String]) -> bool {
    let ext = ext.to_ascii_lowercase();
    !(ext == "mp3" || ext == "wav" || native.iter().any(|n| n.eq_ignore_ascii_case(&ext)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn scan_finds_nested_audio_and_skips_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("Artist/Album")).unwrap();
        fs::create_dir_all(root.join(".hidden")).unwrap();
        File::create(root.join("a.mp3")).unwrap();
        File::create(root.join("Artist/Album/b.FLAC")).unwrap();
        File::create(root.join("Artist/cover.jpg")).unwrap();
        File::create(root.join("Artist/notes.txt")).unwrap();
        File::create(root.join(".hidden/secret.mp3")).unwrap();
        File::create(root.join("._a.mp3")).unwrap();

        let found = scan_source(root).unwrap();
        let names: Vec<_> = found
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(found.len(), 2, "{names:?}");
        assert!(names.contains(&"a.mp3".to_string()));
        assert!(names.contains(&"b.FLAC".to_string()));
        assert_eq!(found.iter().find(|f| f.ext == "flac").unwrap().ext, "flac");
    }

    #[test]
    fn scan_rejects_a_file_as_source() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("x.mp3");
        File::create(&f).unwrap();
        assert!(scan_source(&f).is_err());
    }

    #[test]
    fn transcode_rules() {
        let none: Vec<String> = vec![];
        let flac = vec!["flac".to_string()];
        assert!(!needs_transcode("mp3", &none));
        assert!(!needs_transcode("WAV", &none));
        assert!(needs_transcode("flac", &none));
        assert!(!needs_transcode("flac", &flac));
        assert!(needs_transcode("m4a", &flac));
    }
}
