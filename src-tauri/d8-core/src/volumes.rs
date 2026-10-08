//! Finding the card. macOS only in practice: it shells out to `diskutil` and
//! `df`, and on any other system simply finds nothing.
//!
//! The rules here are what stand between a wipe and the wrong drive, so they
//! are strict: only external or removable media is ever listed, only direct
//! children of /Volumes qualify, and nothing larger than a plausible memory
//! card is offered.

use std::{fs, path::Path, process::Command};

use serde::Serialize;

/// The biggest microSD cards are 1-2 TB, but a speaker like this will not
/// read one. Anything over this is an external drive, not a card.
pub const MAX_CARD_BYTES: u64 = 600 * 1000 * 1000 * 1000;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DiskInfo {
    pub device_id: String,
    pub whole_disk: String,
    pub volume_name: String,
    pub mount_point: String,
    pub personality: String,
    pub protocol: String,
    pub location: String,
    pub removable: bool,
}

impl DiskInfo {
    pub fn is_fat32(&self) -> bool {
        self.personality.to_ascii_lowercase().contains("fat32")
    }

    /// External or removable media: an SD card, a USB stick, a card reader.
    pub fn is_external_media(&self) -> bool {
        let proto = self.protocol.to_ascii_lowercase();
        self.removable
            || self.location.eq_ignore_ascii_case("external")
            || proto.contains("usb")
            || proto.contains("secure digital")
    }
}

/// Parse the human-readable output of `diskutil info <volume>`.
pub fn parse_diskutil_info(text: &str) -> DiskInfo {
    let mut info = DiskInfo::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "Device Identifier" => info.device_id = value.to_string(),
            "Part of Whole" => info.whole_disk = value.to_string(),
            "Volume Name" => info.volume_name = value.to_string(),
            "Mount Point" => info.mount_point = value.to_string(),
            "File System Personality" => info.personality = value.to_string(),
            "Protocol" => info.protocol = value.to_string(),
            "Device Location" => info.location = value.to_string(),
            "Removable Media" => {
                let v = value.to_ascii_lowercase();
                info.removable = v.starts_with("removable") || v == "yes";
            }
            _ => {}
        }
    }
    info
}

/// Parse `df -Pk <path>` into (total bytes, available bytes).
pub fn parse_df(text: &str) -> Option<(u64, u64)> {
    let line = text.lines().nth(1)?;
    let mut cols = line.split_whitespace();
    let _device = cols.next()?;
    let blocks: u64 = cols.next()?.parse().ok()?;
    let _used = cols.next()?;
    let avail: u64 = cols.next()?.parse().ok()?;
    Some((blocks * 1024, avail * 1024))
}

pub fn df_bytes(path: &Path) -> Result<(u64, u64), String> {
    let out = Command::new("df")
        .arg("-Pk")
        .arg(path)
        .output()
        .map_err(|e| format!("could not run df: {e}"))?;
    parse_df(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| "could not read the card's size".to_string())
}

fn diskutil_info(target: &str) -> Result<DiskInfo, String> {
    let out = Command::new("diskutil")
        .args(["info", target])
        .output()
        .map_err(|e| format!("could not run diskutil: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(parse_diskutil_info(&String::from_utf8_lossy(&out.stdout)))
}

#[derive(Debug, Clone, Serialize)]
pub struct Volume {
    pub name: String,
    pub mount_point: String,
    pub device_id: String,
    pub whole_disk: String,
    pub filesystem: String,
    pub is_fat32: bool,
    pub total_bytes: u64,
    pub free_bytes: u64,
    /// Bigger than any card this tool will touch.
    pub too_large: bool,
}

fn volume_from(info: &DiskInfo) -> Option<Volume> {
    if info.mount_point.is_empty() || !info.is_external_media() {
        return None;
    }
    let (total, free) = df_bytes(Path::new(&info.mount_point)).unwrap_or((0, 0));
    Some(Volume {
        name: info.volume_name.clone(),
        mount_point: info.mount_point.clone(),
        device_id: info.device_id.clone(),
        whole_disk: info.whole_disk.clone(),
        filesystem: info.personality.clone(),
        is_fat32: info.is_fat32(),
        total_bytes: total,
        free_bytes: free,
        too_large: total > MAX_CARD_BYTES,
    })
}

/// Every mounted external or removable volume.
pub fn list_volumes() -> Vec<Volume> {
    let Ok(entries) = fs::read_dir("/Volumes") else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        // The boot volume shows up here as a symlink to "/".
        if fs::symlink_metadata(&path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(true)
        {
            continue;
        }
        let Ok(info) = diskutil_info(&path.to_string_lossy()) else {
            continue;
        };
        if let Some(v) = volume_from(&info) {
            out.push(v);
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out
}

/// Look up a single volume by its mount point, re-checking everything.
pub fn volume_at(mount_point: &str) -> Option<Volume> {
    diskutil_info(mount_point)
        .ok()
        .and_then(|info| volume_from(&info))
}

/// Only letters, digits, space, dash and underscore; upper-case; at most
/// eleven characters. That is what a FAT volume label allows.
pub fn sanitize_label(label: &str) -> String {
    let cleaned: String = label
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect::<String>()
        .trim()
        .to_ascii_uppercase();
    let cleaned: String = cleaned.chars().take(11).collect();
    if cleaned.is_empty() {
        "MUSIC".to_string()
    } else {
        cleaned
    }
}

fn is_whole_disk_id(id: &str) -> bool {
    id.strip_prefix("disk")
        .map(|rest| !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit()))
        .unwrap_or(false)
}

/// Erase the whole card and give it a single FAT32 volume on an MBR
/// partition map. MBR matters: small players often cannot read the GPT map
/// that macOS puts on a card by default.
pub fn erase_fat32(whole_disk: &str, label: &str) -> Result<Volume, String> {
    if !is_whole_disk_id(whole_disk) {
        return Err(format!("{whole_disk:?} is not a whole-disk identifier"));
    }

    // Re-check the disk itself, not just what the caller said about it.
    let info = diskutil_info(whole_disk)?;
    if !info.is_external_media() {
        return Err("That disk is not external or removable media. Refusing to erase it.".into());
    }
    let mount_ok = info
        .mount_point
        .is_empty() // a whole disk with no mounted volume reports none
        || info.mount_point.starts_with("/Volumes/");
    if !mount_ok {
        return Err("That disk is mounted somewhere other than /Volumes. Refusing.".into());
    }

    let label = sanitize_label(label);
    let out = Command::new("diskutil")
        .args(["eraseDisk", "FAT32", &label, "MBR", whole_disk])
        .output()
        .map_err(|e| format!("could not run diskutil: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let msg = String::from_utf8_lossy(&out.stdout);
        return Err(format!("diskutil could not format the card. {err}{msg}")
            .trim()
            .to_string());
    }

    // The new volume remounts under /Volumes/<label>, or with a number added
    // if that name is taken. Find it by the disk it lives on.
    list_volumes()
        .into_iter()
        .find(|v| v.whole_disk == whole_disk)
        .ok_or_else(|| "The card was formatted but did not remount.".to_string())
}

pub fn eject(mount_point: &str) -> Result<(), String> {
    let out = Command::new("diskutil")
        .args(["eject", mount_point])
        .output()
        .map_err(|e| format!("could not run diskutil: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SD_FAT32: &str = "
   Device Identifier:         disk4s1
   Device Node:               /dev/disk4s1
   Whole:                     No
   Part of Whole:             disk4

   Volume Name:               MUSIC
   Mounted:                   Yes
   Mount Point:               /Volumes/MUSIC

   File System Personality:   MS-DOS FAT32
   Type (Bundle):             msdos
   Name (User Visible):       MS-DOS (FAT32)

   Protocol:                  Secure Digital
   Device Location:           External
   Removable Media:           Removable
";

    const INTERNAL_APFS: &str = "
   Device Identifier:         disk3s5
   Part of Whole:             disk3
   Volume Name:               Macintosh HD - Data
   Mount Point:               /System/Volumes/Data
   File System Personality:   APFS
   Protocol:                  Apple Fabric
   Device Location:           Internal
   Removable Media:           Fixed
";

    const EXFAT_USB: &str = "
   Device Identifier:         disk5s2
   Part of Whole:             disk5
   Volume Name:               STICK
   Mount Point:               /Volumes/STICK
   File System Personality:   ExFAT
   Protocol:                  USB
   Device Location:           External
   Removable Media:           Removable
";

    #[test]
    fn parses_an_sd_card() {
        let i = parse_diskutil_info(SD_FAT32);
        assert_eq!(i.device_id, "disk4s1");
        assert_eq!(i.whole_disk, "disk4");
        assert_eq!(i.volume_name, "MUSIC");
        assert_eq!(i.mount_point, "/Volumes/MUSIC");
        assert!(i.is_fat32());
        assert!(i.removable);
        assert!(i.is_external_media());
    }

    #[test]
    fn internal_disks_are_never_external_media() {
        let i = parse_diskutil_info(INTERNAL_APFS);
        assert!(!i.is_fat32());
        assert!(!i.is_external_media());
    }

    #[test]
    fn exfat_is_not_fat32() {
        let i = parse_diskutil_info(EXFAT_USB);
        assert!(!i.is_fat32());
        assert!(i.is_external_media());
    }

    #[test]
    fn parses_df() {
        let text = "Filesystem 1024-blocks Used Available Capacity Mounted on\n\
                    /dev/disk4s1 31154688 123 31154565 1% /Volumes/MUSIC CARD\n";
        assert_eq!(parse_df(text), Some((31154688 * 1024, 31154565 * 1024)));
        assert_eq!(parse_df("nonsense"), None);
    }

    #[test]
    fn labels_are_made_safe() {
        assert_eq!(sanitize_label("music"), "MUSIC");
        assert_eq!(sanitize_label("my/very:long*speaker card"), "MYVERYLONGS");
        assert_eq!(sanitize_label("???"), "MUSIC");
    }

    #[test]
    fn whole_disk_ids() {
        assert!(is_whole_disk_id("disk4"));
        assert!(!is_whole_disk_id("disk4s1"));
        assert!(!is_whole_disk_id("/dev/disk4"));
        assert!(!is_whole_disk_id("disk"));
    }
}
