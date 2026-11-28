//! Hamlib `rigctld` network protocol: https://hamlib.sourceforge.net/html/rigctld.1.html

use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;

const TIMEOUT: Duration = Duration::from_secs(2);

pub struct Client {
    stream: BufReader<TcpStream>,
}

impl Client {
    pub async fn connect(host: &str, port: u16) -> Result<Self, String> {
        let stream = timeout(TIMEOUT, TcpStream::connect((host, port)))
            .await
            .map_err(|_| format!("rigctld at {host}:{port} did not answer"))?
            .map_err(|e| format!("rigctld at {host}:{port}: {e}"))?;
        Ok(Client {
            stream: BufReader::new(stream),
        })
    }

    async fn read_line(&mut self) -> Result<String, String> {
        let mut line = String::new();
        let n = timeout(TIMEOUT, self.stream.read_line(&mut line))
            .await
            .map_err(|_| "the radio didn't answer; check it's switched on and the baud rate".to_string())?
            .map_err(|e| e.to_string())?;
        if n == 0 {
            return Err("rigctld closed the connection".into());
        }
        let line = line.trim().to_string();
        if let Some(code) = line.strip_prefix("RPRT ") {
            return Err(rprt_text(code));
        }
        Ok(line)
    }

    async fn send(&mut self, cmd: &str) -> Result<(), String> {
        timeout(TIMEOUT, self.stream.get_mut().write_all(cmd.as_bytes()))
            .await
            .map_err(|_| "rigctld timed out".to_string())?
            .map_err(|e| e.to_string())
    }

    pub async fn poll(&mut self) -> Result<(u64, String), String> {
        self.send("f\n").await?;
        let freq = self.read_line().await?;
        let hz = freq
            .parse::<f64>()
            .map_err(|_| format!("Unexpected frequency from rigctld: {freq}"))? as u64;
        self.send("m\n").await?;
        let mode = self.read_line().await?;
        let _passband = self.read_line().await?;
        Ok((hz, mode))
    }
}

/// Hamlib's error codes, from rig.h.
fn rprt_text(code: &str) -> String {
    let what = match code.trim() {
        "-5" => "the radio didn't answer; is it switched on?",
        "-6" => "I/O error talking to the radio",
        "-8" => "the radio sent something unexpected; check the baud rate",
        "-11" => "the radio doesn't support that",
        _ => return format!("rigctld error {}", code.trim()),
    };
    format!("rigctld: {what}")
}
