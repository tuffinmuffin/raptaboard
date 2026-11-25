//! A plugin's saved values (`storage-get`, `storage-set`): small things it
//! keeps between runs, like a setting or a recent file's path. One folder
//! per plugin, a file per key, all of them together under a size limit.
//!
//! Larger data (saved images, recordings) would get a writable folder of the
//! plugin's own at `/data` instead, beside the read-only `/pack`: not yet.

use std::path::{Path, PathBuf};

/// The most a plugin's values may take together unless its plugin.toml
/// asks for more (`[limits] storage_kb`), in KiB.
pub const DEFAULT_STORAGE_KB: u32 = 64;
/// The most a plugin may ask for, in KiB: storage is for small things.
pub const MAX_STORAGE_KB: u32 = 1024;
/// The longest key.
pub const MAX_KEY_LEN: usize = 64;

/// One plugin's saved values: files in `dir`, at most `limit` bytes in all.
#[derive(Debug, Clone)]
pub struct Store {
    dir: PathBuf,
    limit: u64,
}

/// Whether `key` names a value: 1 to [`MAX_KEY_LEN`] ASCII letters, digits,
/// `-`, `_` and `.`, not starting with `.` (so it's always a plain file name,
/// never a path or a hidden file).
///
/// ```
/// use raptaboard_plugin_host::store::check_key;
/// assert!(check_key("recent-files.v2").is_ok());
/// assert!(check_key("").is_err());
/// assert!(check_key("../x").is_err());
/// assert!(check_key(".hidden").is_err());
/// assert!(check_key("a/b").is_err());
/// ```
pub fn check_key(key: &str) -> Result<(), String> {
    let ok = (1..=MAX_KEY_LEN).contains(&key.len())
        && !key.starts_with('.')
        && key
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "bad key {key:?}: 1 to {MAX_KEY_LEN} letters, digits, '-', '_' or '.' (not first)"
        ))
    }
}

/// The folder name for a plugin's values: its name, with anything but
/// letters, digits, `-`, `_` and `.` made `_`, and never `.` or `..`.
///
/// ```
/// use raptaboard_plugin_host::store::folder_name;
/// assert_eq!(folder_name("doom"), "doom");
/// assert_eq!(folder_name("CAD viewer/2"), "CAD_viewer_2");
/// assert_eq!(folder_name(".."), "_..");
/// assert_eq!(folder_name(""), "_");
/// ```
pub fn folder_name(plugin: &str) -> String {
    let name: String = plugin
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name.starts_with('.') {
        format!("_{name}")
    } else {
        name
    }
}

impl Store {
    /// The values in `dir` (made when first saved to), at most `limit_bytes`.
    pub fn new(dir: impl Into<PathBuf>, limit_bytes: u64) -> Store {
        Store {
            dir: dir.into(),
            limit: limit_bytes,
        }
    }

    /// Where the values are kept.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The value saved under `key`, if any.
    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>, String> {
        check_key(key)?;
        match std::fs::read(self.dir.join(key)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("couldn't read {key}: {e}")),
        }
    }

    /// Save `value` under `key`, replacing what was there, if all the values
    /// still fit the limit. Written whole or not at all.
    pub fn set(&self, key: &str, value: &[u8]) -> Result<(), String> {
        check_key(key)?;
        let others: u64 = self
            .entries()
            .filter(|(k, _)| k != key)
            .map(|(_, size)| size)
            .sum();
        let total = others + value.len() as u64;
        if total > self.limit {
            return Err(format!(
                "storage full: {total} bytes, over its {} KiB",
                self.limit / 1024
            ));
        }
        std::fs::create_dir_all(&self.dir).map_err(|e| format!("couldn't save {key}: {e}"))?;
        // A hidden file first, then renamed over: a crash leaves the old
        // value, never half of the new one.
        let partial = self.dir.join(format!(".{key}.partial"));
        std::fs::write(&partial, value)
            .and_then(|()| std::fs::rename(&partial, self.dir.join(key)))
            .map_err(|e| {
                let _ = std::fs::remove_file(&partial);
                format!("couldn't save {key}: {e}")
            })
    }

    /// Forget `key`'s value (nothing if there's none).
    pub fn remove(&self, key: &str) -> Result<(), String> {
        check_key(key)?;
        match std::fs::remove_file(self.dir.join(key)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("couldn't remove {key}: {e}")),
        }
    }

    /// Every key with a value, in order.
    pub fn keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.entries().map(|(k, _)| k).collect();
        keys.sort();
        keys
    }

    /// Each value's key and size.
    fn entries(&self) -> impl Iterator<Item = (String, u64)> {
        std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| {
                let key = e.file_name().into_string().ok()?;
                let meta = e.metadata().ok()?;
                (meta.is_file() && check_key(&key).is_ok()).then_some((key, meta.len()))
            })
    }
}

#[cfg(test)]
mod tests {
    // Tests written by Claude.
    use super::*;

    fn store(limit: u64) -> (Store, PathBuf) {
        let dir =
            std::env::temp_dir().join(format!("raptaboard-store-{}-{}", std::process::id(), limit));
        let _ = std::fs::remove_dir_all(&dir);
        (Store::new(&dir, limit), dir)
    }

    #[test]
    fn values_are_kept_replaced_and_removed() {
        let (s, dir) = store(1000);
        assert_eq!(s.get("recent").unwrap(), None);
        assert!(s.keys().is_empty(), "no folder yet: nothing");
        s.set("recent", b"/robot/arm.step").unwrap();
        s.set("units", b"mm").unwrap();
        // Another store on the same folder (the next run) sees them.
        let again = Store::new(&dir, 1000);
        assert_eq!(
            again.get("recent").unwrap().as_deref(),
            Some(&b"/robot/arm.step"[..])
        );
        assert_eq!(again.keys(), ["recent", "units"]);
        s.set("recent", b"").unwrap();
        assert_eq!(s.get("recent").unwrap(), Some(Vec::new()));
        s.remove("recent").unwrap();
        s.remove("recent").unwrap();
        assert_eq!(s.keys(), ["units"]);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn all_the_values_together_stay_under_the_limit() {
        let (s, dir) = store(10);
        s.set("a", &[0; 6]).unwrap();
        assert!(s.set("b", &[0; 5]).unwrap_err().contains("storage full"));
        assert_eq!(s.keys(), ["a"], "not saved");
        // Replacing a value counts the new size, not both.
        s.set("a", &[0; 10]).unwrap();
        s.set("a", &[0; 2]).unwrap();
        s.set("b", &[0; 8]).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_key_is_never_a_path() {
        let (s, dir) = store(100);
        for bad in ["../escape", "a/b", "a\\b", ".partial", "", &"k".repeat(65)] {
            assert!(s.set(bad, b"x").is_err(), "{bad:?}");
            assert!(s.get(bad).is_err(), "{bad:?}");
            assert!(s.remove(bad).is_err(), "{bad:?}");
        }
        assert!(!dir.exists(), "nothing written");
        // A save's leftover partial file isn't a value, nor counted.
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".x.partial"), [0; 50]).unwrap();
        assert!(s.keys().is_empty());
        s.set("x", &[0; 100]).unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
