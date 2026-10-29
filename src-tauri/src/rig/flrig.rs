//! FLRig XML-RPC interface (default port 12345).

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;

const TIMEOUT: Duration = Duration::from_secs(2);

pub struct Client {
    host: String,
    port: u16,
}

/// The first `<value>` in an XML-RPC response, with any type tags removed.
fn parse_value(xml: &str) -> Option<String> {
    let start = xml.find("<value>")? + "<value>".len();
    let end = start + xml[start..].find("</value>")?;
    let mut out = String::new();
    let mut in_tag = false;
    for c in xml[start..end].chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    Some(out.trim().to_string())
}

fn content_length(headers: &str) -> Option<usize> {
    headers.lines().find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim()
            .eq_ignore_ascii_case("content-length")
            .then(|| v.trim().parse().ok())
            .flatten()
    })
}

impl Client {
    pub async fn connect(host: &str, port: u16) -> Result<Self, String> {
        let client = Client {
            host: host.to_string(),
            port,
        };
        client.call("rig.get_xcvr").await?;
        Ok(client)
    }

    async fn call(&self, method: &str) -> Result<String, String> {
        let body = format!(
            "<?xml version=\"1.0\"?><methodCall><methodName>{method}</methodName><params></params></methodCall>"
        );
        let request = format!(
            "POST /RPC2 HTTP/1.1\r\nHost: {}:{}\r\nContent-Type: text/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.host,
            self.port,
            body.len()
        );
        let exchange = async {
            let mut stream = TcpStream::connect((self.host.as_str(), self.port))
                .await
                .map_err(|e| format!("FLRig at {}:{}: {e}", self.host, self.port))?;
            stream.write_all(request.as_bytes()).await.map_err(|e| e.to_string())?;
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            loop {
                let n = stream.read(&mut chunk).await.map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(split) = text.find("\r\n\r\n")
                    && let Some(len) = content_length(&text[..split])
                    && buf.len() >= split + 4 + len
                {
                    break;
                }
            }
            Ok::<_, String>(String::from_utf8_lossy(&buf).into_owned())
        };
        let response = timeout(TIMEOUT, exchange)
            .await
            .map_err(|_| format!("FLRig at {}:{} did not answer", self.host, self.port))??;
        if response.contains("<fault>") {
            return Err(format!("FLRig error calling {method}"));
        }
        parse_value(&response).ok_or_else(|| format!("Unexpected reply from FLRig to {method}"))
    }

    pub async fn poll(&mut self) -> Result<(u64, String), String> {
        let freq = self.call("rig.get_vfo").await?;
        let hz = freq
            .parse::<f64>()
            .map_err(|_| format!("Unexpected frequency from FLRig: {freq}"))? as u64;
        let mode = self.call("rig.get_mode").await?;
        Ok((hz, mode))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values() {
        let xml = "<?xml version=\"1.0\"?><methodResponse><params><param><value><string>USB</string></value></param></params></methodResponse>";
        assert_eq!(parse_value(xml).as_deref(), Some("USB"));
        let xml = "<methodResponse><params><param><value>14074000</value></param></params></methodResponse>";
        assert_eq!(parse_value(xml).as_deref(), Some("14074000"));
        assert_eq!(content_length("HTTP/1.1 200 OK\r\nContent-Length: 42"), Some(42));
    }
}
