//! Each running copy keeps its own radio settings. The first copy uses the
//! app config directory. Another copy finds that lock held and takes the next
//! free directory under `instances/`, with no name to type.

use crate::rig::RigConfig;
use crate::settings::Settings;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

const SLOTS: u32 = 20;

pub struct Instance {
    /// Held so this copy keeps the directory until it exits.
    #[allow(dead_code)]
    lock: Option<LockFile>,
    /// This copy's settings, queue and client number.
    pub files_dir: PathBuf,
}

struct LockFile {
    path: PathBuf,
}

impl Drop for LockFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

/// Take the first free settings directory. Holds the lock until dropped.
pub fn claim(config_dir: &Path) -> Instance {
    let _ = fs::create_dir_all(config_dir);
    if let Some(lock) = try_lock(&config_dir.join("instance.lock")) {
        return Instance { lock: Some(lock), files_dir: config_dir.to_path_buf() };
    }
    for n in 2..=SLOTS {
        let dir = config_dir.join("instances").join(n.to_string());
        let _ = fs::create_dir_all(&dir);
        if let Some(lock) = try_lock(&dir.join("instance.lock")) {
            seed_settings(config_dir, &dir);
            return Instance { lock: Some(lock), files_dir: dir };
        }
    }
    eprintln!("openvkcl: every radio slot is in use; this copy shares the first settings file");
    Instance { lock: None, files_dir: config_dir.to_path_buf() }
}

fn try_lock(path: &Path) -> Option<LockFile> {
    if let Some(lock) = create_lock(path) {
        return Some(lock);
    }
    if owner_dead(path) {
        let _ = fs::remove_file(path);
        return create_lock(path);
    }
    None
}

fn create_lock(path: &Path) -> Option<LockFile> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).ok()?;
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(path).ok()?;
    let _ = writeln!(file, "{}", std::process::id());
    Some(LockFile { path: path.to_path_buf() })
}

fn owner_dead(path: &Path) -> bool {
    let Ok(text) = fs::read_to_string(path) else { return true };
    let Ok(pid) = text.trim().parse::<i32>() else { return true };
    if pid <= 0 {
        return true;
    }
    // Our own lock is held by this process. kill(pid, 0) would also say we are alive.
    if pid as u32 == std::process::id() {
        return false;
    }
    process_gone(pid)
}

#[cfg(unix)]
fn process_gone(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) != 0 }
}

#[cfg(not(unix))]
fn process_gone(pid: i32) -> bool {
    let _ = pid;
    false
}

/// A new copy starts from the station details already entered, without the
/// other copy's radio or its open log.
fn seed_settings(shared: &Path, dir: &Path) {
    let dest = dir.join("settings.json");
    if dest.exists() {
        return;
    }
    let mut settings = Settings::load(&shared.join("settings.json"));
    settings.rig = RigConfig::default();
    settings.last_log = None;
    let _ = settings.save(&dest);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig::RigKind;

    #[test]
    fn a_second_copy_keeps_its_own_radio() {
        let dir = std::env::temp_dir().join(format!("openvkcl-instance-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let first = claim(&dir);
        assert_eq!(first.files_dir, dir);
        let mut settings = Settings::default();
        settings.station.callsign = "ZL2BRG".into();
        settings.rig.kind = RigKind::Hamlib;
        settings.rig.serial_port = "/dev/radio-a".into();
        settings.last_log = Some("field.ovkcl".into());
        settings.save(&dir.join("settings.json")).unwrap();

        let second = claim(&dir);
        assert_eq!(second.files_dir, dir.join("instances/2"));
        let seeded = Settings::load(&second.files_dir.join("settings.json"));
        assert_eq!(seeded.station.callsign, "ZL2BRG");
        assert_eq!(seeded.rig.kind, RigKind::None);
        assert!(seeded.last_log.is_none());

        drop(second);
        let mut picked = Settings::load(&dir.join("instances/2/settings.json"));
        picked.rig.kind = RigKind::Hamlib;
        picked.rig.serial_port = "/dev/radio-b".into();
        picked.save(&dir.join("instances/2/settings.json")).unwrap();
        let again = claim(&dir);
        let kept = Settings::load(&again.files_dir.join("settings.json"));
        assert_eq!(kept.rig.serial_port, "/dev/radio-b");
        let _ = fs::remove_dir_all(&dir);
    }
}
