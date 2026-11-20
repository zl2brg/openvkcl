//! QRZ.com XML callsign lookups. Full details need a QRZ XML subscription;
//! free accounts get name and country only.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::sync::Mutex;

const API_URL: &str = "https://xmldata.qrz.com/xml/current/";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct QrzConfig {
    pub enabled: bool,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct QrzInfo {
    pub call: String,
    pub name: String,
    pub city: String,
    pub state: String,
    pub country: String,
    pub grid: String,
    pub email: String,
    pub lat: Option<f64>,
    pub lon: Option<f64>,
}

#[derive(Default)]
pub struct Qrz {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    /// Session key, and the credentials it was issued for.
    session: Option<(String, String, String)>,
    /// Lookups this run; `None` means QRZ has no record.
    cache: HashMap<String, Option<QrzInfo>>,
}

/// Text of the first `<tag>…</tag>` in `xml`, entity-decoded.
fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    let start = xml.find(&open)? + open.len();
    let len = xml[start..].find(&close)?;
    let text = xml[start..start + len]
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&");
    Some(text.trim().to_string())
}

fn parse_info(xml: &str) -> Option<QrzInfo> {
    let rec = tag(xml, "Callsign")?;
    let get = |n: &str| tag(&rec, n).unwrap_or_default();
    let name = [get("fname"), get("name")]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    Some(QrzInfo {
        call: get("call").to_ascii_uppercase(),
        name,
        city: get("addr2"),
        state: get("state"),
        country: get("country"),
        grid: get("grid"),
        email: get("email"),
        lat: get("lat").parse().ok(),
        lon: get("lon").parse().ok(),
    })
}

async fn get(params: &[(&str, &str)]) -> Result<String, String> {
    let resp = reqwest::Client::new()
        .get(API_URL)
        .query(params)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| format!("QRZ.com: {e}"))?;
    resp.text().await.map_err(|e| format!("QRZ.com: {e}"))
}

async fn login(cfg: &QrzConfig, agent: &str) -> Result<String, String> {
    if cfg.username.trim().is_empty() || cfg.password.is_empty() {
        return Err("Enter your QRZ.com username and password in Lookups".into());
    }
    let xml = get(&[
        ("username", cfg.username.trim()),
        ("password", cfg.password.as_str()),
        ("agent", agent),
    ])
    .await?;
    match tag(&xml, "Key") {
        Some(key) if !key.is_empty() => Ok(key),
        _ => Err(format!(
            "QRZ.com login failed: {}",
            tag(&xml, "Error").unwrap_or_else(|| "no session key returned".into())
        )),
    }
}

impl Qrz {
    /// Log in and report the subscription expiry, to check the settings.
    pub async fn test(&self, cfg: &QrzConfig, agent: &str) -> Result<String, String> {
        let key = login(cfg, agent).await?;
        let mut inner = self.inner.lock().await;
        inner.session = Some((cfg.username.clone(), cfg.password.clone(), key.clone()));
        inner.cache.clear();
        let xml = get(&[("s", key.as_str())]).await?;
        Ok(match tag(&xml, "SubExp") {
            Some(exp) if exp != "non-subscriber" => format!("Logged in; XML subscription expires {exp}"),
            _ => "Logged in without an XML subscription: lookups give name and country only".into(),
        })
    }

    pub async fn lookup(&self, cfg: &QrzConfig, agent: &str, call: &str) -> Result<Option<QrzInfo>, String> {
        let call = call.trim().to_ascii_uppercase();
        let mut inner = self.inner.lock().await;
        if let Some(hit) = inner.cache.get(&call) {
            return Ok(hit.clone());
        }
        for attempt in 0..2 {
            let current = match &inner.session {
                Some((u, p, k)) if *u == cfg.username && *p == cfg.password && attempt == 0 => Some(k.clone()),
                _ => None,
            };
            let key = match current {
                Some(k) => k,
                None => {
                    let k = login(cfg, agent).await?;
                    inner.session = Some((cfg.username.clone(), cfg.password.clone(), k.clone()));
                    k
                }
            };
            let xml = get(&[("s", key.as_str()), ("callsign", call.as_str())]).await?;
            if let Some(info) = parse_info(&xml) {
                inner.cache.insert(call, Some(info.clone()));
                return Ok(Some(info));
            }
            let err = tag(&xml, "Error").unwrap_or_default();
            if err.starts_with("Not found") {
                inner.cache.insert(call, None);
                return Ok(None);
            }
            // An expired or invalid session has no key; log in again once.
            if tag(&xml, "Key").is_some_and(|k| !k.is_empty()) || attempt == 1 {
                return Err(format!("QRZ.com: {}", if err.is_empty() { "unexpected reply" } else { &err }));
            }
            inner.session = None;
        }
        unreachable!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_record() {
        let xml = r#"<?xml version="1.0" ?>
<QRZDatabase version="1.34" xmlns="http://xmldata.qrz.com">
<Callsign><call>vk4abc</call><fname>Jo &amp; Sam</fname><name>Citizen</name>
<addr2>Brisbane</addr2><state>QLD</state><country>Australia</country>
<grid>QG62lm</grid><lat>-27.47</lat><lon>153.02</lon></Callsign>
<Session><Key>abc123</Key></Session></QRZDatabase>"#;
        let i = parse_info(xml).unwrap();
        assert_eq!(i.call, "VK4ABC");
        assert_eq!(i.name, "Jo & Sam Citizen");
        assert_eq!(i.city, "Brisbane");
        assert_eq!(i.grid, "QG62lm");
        assert_eq!(i.lat, Some(-27.47));
        assert_eq!(tag(xml, "Key").as_deref(), Some("abc123"));
    }

    #[test]
    fn no_record() {
        let xml = "<QRZDatabase><Session><Error>Not found: VK9ZZZ</Error><Key>k</Key></Session></QRZDatabase>";
        assert!(parse_info(xml).is_none());
        assert_eq!(tag(xml, "Error").as_deref(), Some("Not found: VK9ZZZ"));
    }
}
