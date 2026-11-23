use crate::bands;
use crate::model::{LogSetup, Qso, QsoDetails};
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::Path;

const APP_TAG: &str = "openvkcl";
/// Raised only when older versions can't safely use the file. Columns that older
/// versions can ignore (they read and write only their own, and new ones have
/// defaults) are added without raising it; see `DETAIL_COLUMNS`.
const SCHEMA_VERSION: i64 = 2;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS qso (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    time TEXT NOT NULL,
    call TEXT NOT NULL,
    band TEXT NOT NULL,
    mode TEXT NOT NULL,
    freq_khz REAL,
    sent_rst TEXT NOT NULL DEFAULT '',
    sent_nr TEXT NOT NULL DEFAULT '',
    sent_locator TEXT NOT NULL DEFAULT '',
    rcvd_rst TEXT NOT NULL DEFAULT '',
    rcvd_nr TEXT NOT NULL DEFAULT '',
    rcvd_locator TEXT NOT NULL DEFAULT '',
    operator TEXT NOT NULL DEFAULT '',
    deleted INTEGER NOT NULL DEFAULT 0,
    sent_exch TEXT NOT NULL DEFAULT '',
    rcvd_exch TEXT NOT NULL DEFAULT '',
    name TEXT NOT NULL DEFAULT '',
    qth TEXT NOT NULL DEFAULT '',
    comment TEXT NOT NULL DEFAULT '',
    dxcc INTEGER,
    qsl_sent TEXT NOT NULL DEFAULT '',
    qsl_rcvd TEXT NOT NULL DEFAULT '',
    lotw_sent TEXT NOT NULL DEFAULT '',
    lotw_rcvd TEXT NOT NULL DEFAULT '',
    eqsl_sent TEXT NOT NULL DEFAULT '',
    eqsl_rcvd TEXT NOT NULL DEFAULT '',
    adif TEXT NOT NULL DEFAULT ''
);
CREATE INDEX IF NOT EXISTS qso_call ON qso(call);
";

const COLUMNS: &str = "id, time, call, band, mode, freq_khz, sent_rst, sent_nr, sent_locator, \
                       rcvd_rst, rcvd_nr, rcvd_locator, operator, sent_exch, rcvd_exch, \
                       name, qth, comment, dxcc, qsl_sent, qsl_rcvd, lotw_sent, lotw_rcvd, \
                       eqsl_sent, eqsl_rcvd, adif";

/// General logging columns, added to older files when they're opened. Older
/// versions of the app ignore them, so they don't raise the schema version.
const DETAIL_COLUMNS: &[(&str, &str)] = &[
    ("name", "TEXT NOT NULL DEFAULT ''"),
    ("qth", "TEXT NOT NULL DEFAULT ''"),
    ("comment", "TEXT NOT NULL DEFAULT ''"),
    ("dxcc", "INTEGER"),
    ("qsl_sent", "TEXT NOT NULL DEFAULT ''"),
    ("qsl_rcvd", "TEXT NOT NULL DEFAULT ''"),
    ("lotw_sent", "TEXT NOT NULL DEFAULT ''"),
    ("lotw_rcvd", "TEXT NOT NULL DEFAULT ''"),
    ("eqsl_sent", "TEXT NOT NULL DEFAULT ''"),
    ("eqsl_rcvd", "TEXT NOT NULL DEFAULT ''"),
    ("adif", "TEXT NOT NULL DEFAULT ''"),
];

/// Test builds of general logging marked files as version 3 for the detail
/// columns; those files are version 2 files that older versions can open.
const DETAIL_TEST_VERSION: i64 = 3;

const INSERT: &str = "INSERT INTO qso (time, call, band, mode, freq_khz, sent_rst, sent_nr, sent_locator,
                                       rcvd_rst, rcvd_nr, rcvd_locator, operator, sent_exch, rcvd_exch,
                                       name, qth, comment, dxcc, qsl_sent, qsl_rcvd, lotw_sent, lotw_rcvd,
                                       eqsl_sent, eqsl_rcvd, adif)
                      VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                              ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)";

pub struct Store {
    conn: Connection,
    /// Schema version the file had before `open` upgraded it.
    pub upgraded_from: Option<i64>,
}

fn err(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn time_str(t: &DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn row_to_qso(row: &Row) -> rusqlite::Result<Qso> {
    let time: String = row.get(1)?;
    let time = DateTime::parse_from_rfc3339(&time)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e)))?;
    Ok(Qso {
        id: row.get(0)?,
        time,
        call: row.get(2)?,
        band: bands::canonical(&row.get::<_, String>(3)?),
        mode: row.get(4)?,
        freq_khz: row.get(5)?,
        sent_rst: row.get(6)?,
        sent_nr: row.get(7)?,
        sent_locator: row.get(8)?,
        rcvd_rst: row.get(9)?,
        rcvd_nr: row.get(10)?,
        rcvd_locator: row.get(11)?,
        operator: row.get(12)?,
        sent_exch: row.get(13)?,
        rcvd_exch: row.get(14)?,
        details: QsoDetails {
            name: row.get(15)?,
            qth: row.get(16)?,
            comment: row.get(17)?,
            dxcc: row.get(18)?,
            qsl_sent: row.get(19)?,
            qsl_rcvd: row.get(20)?,
            lotw_sent: row.get(21)?,
            lotw_rcvd: row.get(22)?,
            eqsl_sent: row.get(23)?,
            eqsl_rcvd: row.get(24)?,
            adif: serde_json::from_str(&row.get::<_, String>(25)?).unwrap_or_default(),
        },
    })
}

fn adif_json(d: &QsoDetails) -> String {
    if d.adif.is_empty() {
        String::new()
    } else {
        serde_json::to_string(&d.adif).unwrap_or_default()
    }
}

fn insert_with(conn: &Connection, q: &Qso) -> rusqlite::Result<i64> {
    let d = &q.details;
    conn.execute(
        INSERT,
        params![
            time_str(&q.time),
            q.call,
            bands::canonical(&q.band),
            q.mode,
            q.freq_khz,
            q.sent_rst,
            q.sent_nr,
            q.sent_locator,
            q.rcvd_rst,
            q.rcvd_nr,
            q.rcvd_locator,
            q.operator,
            q.sent_exch,
            q.rcvd_exch,
            d.name,
            d.qth,
            d.comment,
            d.dxcc,
            d.qsl_sent,
            d.qsl_rcvd,
            d.lotw_sent,
            d.lotw_rcvd,
            d.eqsl_sent,
            d.eqsl_rcvd,
            adif_json(d)
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

impl Store {
    pub fn create(path: &Path, setup: &LogSetup) -> Result<Self, String> {
        if path.exists() {
            std::fs::remove_file(path).map_err(err)?;
        }
        let conn = Connection::open(path).map_err(err)?;
        conn.execute_batch(SCHEMA).map_err(err)?;
        let store = Store { conn, upgraded_from: None };
        store.set_meta("app", APP_TAG)?;
        store.set_meta("schema", &SCHEMA_VERSION.to_string())?;
        store.save_setup(setup)?;
        Ok(store)
    }

    pub fn open(path: &Path) -> Result<(Self, LogSetup), String> {
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
        let conn = Connection::open(path).map_err(err)?;
        let mut store = Store { conn, upgraded_from: None };
        let app = store.meta("app").map_err(|_| "Not an OpenVKCL log file".to_string())?;
        if app.as_deref() != Some(APP_TAG) {
            return Err("Not an OpenVKCL log file".into());
        }
        let mut version: i64 = store
            .meta("schema")?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if version == DETAIL_TEST_VERSION {
            store.set_meta("schema", &SCHEMA_VERSION.to_string())?;
            version = SCHEMA_VERSION;
        }
        if version > SCHEMA_VERSION {
            return Err("This log was created by a newer version of OpenVKCL".into());
        }
        if version < 2 {
            store
                .conn
                .execute_batch(
                    "ALTER TABLE qso ADD COLUMN sent_exch TEXT NOT NULL DEFAULT '';
                     ALTER TABLE qso ADD COLUMN rcvd_exch TEXT NOT NULL DEFAULT '';",
                )
                .map_err(err)?;
        }
        store.add_detail_columns()?;
        if version < SCHEMA_VERSION {
            store.set_meta("schema", &SCHEMA_VERSION.to_string())?;
            store.upgraded_from = Some(version);
        }
        let setup = store
            .meta("setup")?
            .ok_or("Log file has no contest setup")?;
        let mut setup: LogSetup = serde_json::from_str(&setup).map_err(err)?;
        for bands in setup.bonus_bands.values_mut() {
            for b in bands.iter_mut() {
                *b = bands::canonical(b);
            }
        }
        Ok((store, setup))
    }

    fn add_detail_columns(&self) -> Result<(), String> {
        let mut stmt = self.conn.prepare("SELECT name FROM pragma_table_info('qso')").map_err(err)?;
        let have: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .map_err(err)?
            .collect::<rusqlite::Result<_>>()
            .map_err(err)?;
        for (name, decl) in DETAIL_COLUMNS {
            if !have.iter().any(|h| h == name) {
                self.conn
                    .execute_batch(&format!("ALTER TABLE qso ADD COLUMN {name} {decl}"))
                    .map_err(err)?;
            }
        }
        Ok(())
    }

    fn meta(&self, key: &str) -> Result<Option<String>, String> {
        self.conn
            .query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0))
            .optional()
            .map_err(err)
    }

    fn set_meta(&self, key: &str, value: &str) -> Result<(), String> {
        self.conn
            .execute(
                "INSERT INTO meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map(|_| ())
            .map_err(err)
    }

    /// Version 1 logs kept the `number` field in the serial columns.
    pub fn move_numbers_to_exch(&self) -> Result<(), String> {
        self.conn
            .execute(
                "UPDATE qso SET sent_exch = sent_nr, rcvd_exch = rcvd_nr, sent_nr = '', rcvd_nr = ''",
                [],
            )
            .map(|_| ())
            .map_err(err)
    }

    pub fn save_setup(&self, setup: &LogSetup) -> Result<(), String> {
        self.set_meta("setup", &serde_json::to_string(setup).map_err(err)?)
    }

    pub fn qsos(&self) -> Result<Vec<Qso>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {COLUMNS} FROM qso WHERE deleted = 0 ORDER BY time, id"
            ))
            .map_err(err)?;
        stmt.query_map([], row_to_qso)
            .map_err(err)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(err)
    }

    pub fn insert(&self, q: &Qso) -> Result<i64, String> {
        insert_with(&self.conn, q).map_err(err)
    }

    /// Insert many contacts in one transaction, setting each one's id.
    pub fn insert_all(&mut self, qsos: &mut [Qso]) -> Result<(), String> {
        let tx = self.conn.transaction().map_err(err)?;
        for q in qsos.iter_mut() {
            q.id = insert_with(&tx, q).map_err(err)?;
        }
        tx.commit().map_err(err)
    }

    pub fn update(&self, q: &Qso) -> Result<(), String> {
        let n = self
            .conn
            .execute(
                "UPDATE qso SET time = ?2, call = ?3, band = ?4, mode = ?5, freq_khz = ?6,
                     sent_rst = ?7, sent_nr = ?8, sent_locator = ?9, rcvd_rst = ?10,
                     rcvd_nr = ?11, rcvd_locator = ?12, operator = ?13,
                     sent_exch = ?14, rcvd_exch = ?15, name = ?16, qth = ?17, comment = ?18,
                     dxcc = ?19, qsl_sent = ?20, qsl_rcvd = ?21, lotw_sent = ?22,
                     lotw_rcvd = ?23, eqsl_sent = ?24, eqsl_rcvd = ?25, adif = ?26
                 WHERE id = ?1 AND deleted = 0",
                params![
                    q.id,
                    time_str(&q.time),
                    q.call,
                    bands::canonical(&q.band),
                    q.mode,
                    q.freq_khz,
                    q.sent_rst,
                    q.sent_nr,
                    q.sent_locator,
                    q.rcvd_rst,
                    q.rcvd_nr,
                    q.rcvd_locator,
                    q.operator,
                    q.sent_exch,
                    q.rcvd_exch,
                    q.details.name,
                    q.details.qth,
                    q.details.comment,
                    q.details.dxcc,
                    q.details.qsl_sent,
                    q.details.qsl_rcvd,
                    q.details.lotw_sent,
                    q.details.lotw_rcvd,
                    q.details.eqsl_sent,
                    q.details.eqsl_rcvd,
                    adif_json(&q.details)
                ],
            )
            .map_err(err)?;
        if n == 0 {
            return Err(format!("QSO {} not found", q.id));
        }
        Ok(())
    }

    /// Deleted contacts stay in the file so a mistaken delete can be recovered.
    pub fn delete(&self, id: i64) -> Result<(), String> {
        self.conn
            .execute("UPDATE qso SET deleted = 1 WHERE id = ?1", [id])
            .map(|_| ())
            .map_err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Station;
    use chrono::TimeZone;

    #[test]
    fn roundtrip() {
        let dir = std::env::temp_dir().join(format!("openvkcl-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.ovkcl");
        let start = Utc.with_ymd_and_hms(2026, 8, 15, 3, 0, 0).unwrap();
        let setup = LogSetup {
            contest_id: "wia-rd".into(),
            start,
            station: Station {
                callsign: "VK3AAA".into(),
                ..Default::default()
            },
            sent_number: "12".into(),
            serial_per_band: false,
            utc_offset_minutes: 600,
            location: String::new(),
            categories: Default::default(),
            bonus_bands: Default::default(),
            soapbox: String::new(),
        };
        let store = Store::create(&path, &setup).unwrap();
        let mut q = Qso {
            id: 0,
            time: start,
            call: "VK2ABC".into(),
            band: "40m".into(),
            mode: "PH".into(),
            freq_khz: Some(7090.0),
            sent_rst: "59".into(),
            sent_nr: "12".into(),
            sent_locator: String::new(),
            rcvd_rst: "59".into(),
            rcvd_nr: "30".into(),
            rcvd_locator: String::new(),
            operator: String::new(),
            sent_exch: "12".into(),
            rcvd_exch: "30".into(),
            details: QsoDetails {
                name: "Bob".into(),
                dxcc: Some(150),
                lotw_rcvd: "Y".into(),
                adif: [("TX_PWR".to_string(), "100".to_string())].into(),
                ..Default::default()
            },
        };
        q.id = store.insert(&q).unwrap();
        q.call = "VK2ABD".into();
        store.update(&q).unwrap();
        let id2 = store.insert(&q).unwrap();
        store.delete(id2).unwrap();
        drop(store);

        let (store, loaded) = Store::open(&path).unwrap();
        assert_eq!(loaded.sent_number, "12");
        let qsos = store.qsos().unwrap();
        assert_eq!(qsos.len(), 1);
        assert_eq!(qsos[0].call, "VK2ABD");
        assert_eq!(qsos[0].time, start);
        assert_eq!(qsos[0].rcvd_exch, "30");
        assert_eq!(qsos[0].details, q.details);
        assert_eq!(store.upgraded_from, None);
        // Older versions check the schema; the detail columns mustn't raise it.
        assert_eq!(store.meta("schema").unwrap().as_deref(), Some("2"));
        // What an older version does: read and write only its own columns.
        store
            .conn
            .execute("UPDATE qso SET call = 'VK2ABE' WHERE id = ?1", [q.id])
            .unwrap();
        store
            .conn
            .execute("INSERT INTO qso (time, call, band, mode) VALUES ('2026-08-15T04:00:00Z', 'VK4AA', '40m', 'PH')", [])
            .unwrap();
        let qsos = store.qsos().unwrap();
        assert_eq!(qsos[0].details, q.details);
        assert_eq!(qsos[1].details, QsoDetails::default());
        store.conn.execute("DELETE FROM qso WHERE call = 'VK4AA'", []).unwrap();
        store.conn.execute("UPDATE qso SET call = 'VK2ABD'", []).unwrap();

        // Files marked version 3 by test builds open, and go back to version 2.
        store.set_meta("schema", "3").unwrap();
        drop(store);
        let (store, _) = Store::open(&path).unwrap();
        assert_eq!(store.meta("schema").unwrap().as_deref(), Some("2"));
        assert_eq!(store.qsos().unwrap()[0].details, q.details);

        // A version 1 file: no exch columns, and the number kept in rcvd_nr.
        store
            .conn
            .execute_batch(
                "ALTER TABLE qso DROP COLUMN sent_exch;
                 ALTER TABLE qso DROP COLUMN rcvd_exch;
                 ALTER TABLE qso DROP COLUMN name;
                 ALTER TABLE qso DROP COLUMN qth;
                 ALTER TABLE qso DROP COLUMN comment;
                 ALTER TABLE qso DROP COLUMN dxcc;
                 ALTER TABLE qso DROP COLUMN qsl_sent;
                 ALTER TABLE qso DROP COLUMN qsl_rcvd;
                 ALTER TABLE qso DROP COLUMN lotw_sent;
                 ALTER TABLE qso DROP COLUMN lotw_rcvd;
                 ALTER TABLE qso DROP COLUMN eqsl_sent;
                 ALTER TABLE qso DROP COLUMN eqsl_rcvd;
                 ALTER TABLE qso DROP COLUMN adif;
                 UPDATE meta SET value = '1' WHERE key = 'schema';",
            )
            .unwrap();
        drop(store);
        let (store, _) = Store::open(&path).unwrap();
        assert_eq!(store.upgraded_from, Some(1));
        store.move_numbers_to_exch().unwrap();
        let qsos = store.qsos().unwrap();
        assert_eq!((qsos[0].rcvd_nr.as_str(), qsos[0].rcvd_exch.as_str()), ("", "30"));
        assert_eq!(qsos[0].details, QsoDetails::default());
        drop(store);
        let (store, _) = Store::open(&path).unwrap();
        assert_eq!(store.upgraded_from, None);
        std::fs::remove_dir_all(dir).ok();
    }
}
