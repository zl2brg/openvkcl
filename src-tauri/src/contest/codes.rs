//! Official lists of exchange codes, such as the WIA's VK Shires list, read from a
//! file the user can replace whenever a new one is published.

use super::def::{ContestDef, norm_code};
use rusqlite::{Connection, OpenFlags};
use std::collections::BTreeMap;
use std::path::Path;

/// Codes and what they stand for, e.g. "BU4" -> "Bundaberg, Qld", from a file in the
/// layout of the WIA's `Shire.db3` for VKCL.
pub fn read(path: &Path) -> Result<BTreeMap<String, String>, String> {
    let fail = |e: rusqlite::Error| format!("{} couldn't be read as a shire list: {e}", path.display());
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(fail)?;
    let mut stmt = db
        .prepare("SELECT shire_shireCode, shire_shireName, shire_stateName FROM shireTbl")
        .map_err(fail)?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .map_err(fail)?;
    let mut codes = BTreeMap::new();
    for row in rows {
        let (code, name, state) = row.map_err(fail)?;
        let code = norm_code(code.as_deref().unwrap_or(""));
        if code.is_empty() {
            continue;
        }
        let name = name.unwrap_or_default().trim().to_string();
        let state = state.unwrap_or_default().trim().to_string();
        let shown = match (name.is_empty(), state.is_empty()) {
            (false, false) => format!("{name}, {state}"),
            (false, true) => name,
            (true, _) => state,
        };
        codes.insert(code, shown);
    }
    if codes.is_empty() {
        return Err(format!("{} has no shires in it", path.display()));
    }
    Ok(codes)
}

/// Read the code list for a contest that has one, from the user's contests folder.
pub fn attach(def: &mut ContestDef, user_dir: Option<&Path>) {
    let Some(list) = &def.number_list else { return };
    let Some(dir) = user_dir else {
        def.number_list_error = Some(format!("No contests folder to find {} in", list.file));
        return;
    };
    let path = dir.join(&list.file);
    if !path.exists() {
        let from = if list.url.is_empty() { String::new() } else { format!(" from {}", list.url) };
        def.number_list_error = Some(format!(
            "{} not found. Download it{from} and put it in {}",
            list.file,
            dir.display()
        ));
        return;
    }
    match read(&path) {
        Ok(codes) => def.number_names = codes,
        Err(e) => def.number_list_error = Some(e),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::path::PathBuf;

    /// An empty folder of our own under the system temp folder.
    pub fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("openvkcl-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A small list in the `Shire.db3` layout, as the WIA publishes it.
    pub fn write_sample(path: &Path) {
        let db = Connection::open(path).unwrap();
        db.execute_batch(
            "CREATE TABLE shireTbl(shire_rowId Integer PRIMARY KEY AUTOINCREMENT, shire_stateId TEXT,
                 shire_stateName TEXT, shire_shireName TEXT, shire_shireCode TEXT);
             INSERT INTO shireTbl (shire_stateId, shire_stateName, shire_shireName, shire_shireCode) VALUES
                 ('4', 'Qld', 'Bundaberg', 'BU4'),
                 ('4', 'Qld', 'Brisbane', 'BN4'),
                 ('5', 'SA ', 'Adelaide', 'AD5'),
                 ('0', ' ', 'Unincorp. Other Territories', 'XX0');",
        )
        .unwrap();
    }

    #[test]
    fn reads_wia_layout() {
        let dir = test_dir("codes-read");
        let path = dir.join("Shire.db3");
        write_sample(&path);
        let codes = read(&path).unwrap();
        assert_eq!(codes.len(), 4);
        assert_eq!(codes["BU4"], "Bundaberg, Qld");
        assert_eq!(codes["AD5"], "Adelaide, SA");
        assert_eq!(codes["XX0"], "Unincorp. Other Territories");
    }

    #[test]
    fn rejects_other_files() {
        let dir = test_dir("codes-reject");
        let path = dir.join("Shire.db3");
        std::fs::write(&path, "not a database").unwrap();
        assert!(read(&path).is_err());
        let empty = dir.join("empty.db3");
        Connection::open(&empty)
            .unwrap()
            .execute_batch("CREATE TABLE shireTbl(shire_stateName TEXT, shire_shireName TEXT, shire_shireCode TEXT);")
            .unwrap();
        assert!(read(&empty).unwrap_err().contains("no shires"));
    }
}
