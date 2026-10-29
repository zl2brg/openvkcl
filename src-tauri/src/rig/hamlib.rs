//! Runs Hamlib's `rigctld` for a radio on a serial or USB port, so the user only picks the
//! radio model and port. `rigctld` comes bundled on Windows and from the system elsewhere.

use super::{RigConfig, rigctld};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

const STARTUP: Duration = Duration::from_secs(10);

/// The `rigctld` started for one connection, shared so it can be killed when the app exits.
#[derive(Clone, Default)]
pub struct Process(Arc<Mutex<Option<Child>>>);

impl Process {
    pub fn kill(&self) {
        if let Some(mut child) = self.0.lock().unwrap().take() {
            let _ = child.start_kill();
        }
    }

    fn exited(&self) -> bool {
        match self.0.lock().unwrap().as_mut() {
            Some(child) => !matches!(child.try_wait(), Ok(None)),
            None => true,
        }
    }
}

pub struct Client {
    rigctld: rigctld::Client,
    process: Process,
    stderr: Arc<Mutex<String>>,
}

impl Drop for Client {
    fn drop(&mut self) {
        self.process.kill();
    }
}

impl Client {
    pub async fn start(cfg: &RigConfig, resource_dir: Option<&Path>, process: Process) -> Result<Self, String> {
        let port_name = cfg.serial_port.trim();
        if cfg.model == 0 || port_name.is_empty() {
            return Err("Choose your radio model and serial port in Radio…".into());
        }
        if cfg!(unix) && !Path::new(port_name).exists() {
            return Err(format!("{port_name} not found; is the radio plugged in and switched on?"));
        }
        let exe = find_rigctld(resource_dir).ok_or_else(not_found)?;
        let tcp_port = std::net::TcpListener::bind("127.0.0.1:0")
            .and_then(|l| l.local_addr())
            .map_err(|e| e.to_string())?
            .port();

        let mut cmd = command(&exe);
        cmd.args(["-m", &cfg.model.to_string(), "-r", port_name])
            .args(["-T", "127.0.0.1", "-t", &tcp_port.to_string()]);
        if cfg.baud > 0 {
            cmd.args(["-s", &cfg.baud.to_string()]);
        }
        cmd.args(cfg.rigctld_args.split_whitespace())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| format!("Couldn't start {}: {e}", exe.display()))?;

        let stderr = Arc::new(Mutex::new(String::new()));
        if let Some(pipe) = child.stderr.take() {
            let stderr = stderr.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(pipe).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if is_message(&line) {
                        *stderr.lock().unwrap() = line.trim().to_string();
                    }
                }
            });
        }
        *process.0.lock().unwrap() = Some(child);

        let deadline = Instant::now() + STARTUP;
        let error = loop {
            if process.exited() {
                break stopped(&stderr).await;
            }
            if let Ok(rigctld) = rigctld::Client::connect("127.0.0.1", tcp_port).await {
                return Ok(Client { rigctld, process, stderr });
            }
            if Instant::now() > deadline {
                break "rigctld started but isn't accepting connections".into();
            }
            tokio::time::sleep(Duration::from_millis(200)).await;
        };
        process.kill();
        Err(error)
    }

    pub async fn poll(&mut self) -> Result<(u64, String), String> {
        match self.rigctld.poll().await {
            Ok(r) => Ok(r),
            Err(_) if self.process.exited() => Err(stopped(&self.stderr).await),
            Err(e) => Err(e),
        }
    }
}

/// Why `rigctld` exited, from the last thing it printed.
async fn stopped(stderr: &Mutex<String>) -> String {
    tokio::time::sleep(Duration::from_millis(200)).await;
    let last = stderr.lock().unwrap().clone();
    if last.is_empty() {
        "rigctld stopped; check the radio model, serial port and baud rate".into()
    } else {
        format!("rigctld stopped: {last}")
    }
}

/// Hamlib 4.5 prints a line per backend as it loads; skip those.
fn is_message(line: &str) -> bool {
    let l = line.trim();
    !l.is_empty() && !l.contains("_init") && !l.starts_with("initrigs")
}

fn command(exe: &Path) -> Command {
    let mut cmd = Command::new(exe);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    // Stop rigctld if the app is killed or crashes, so it can't hold the serial port open.
    #[cfg(target_os = "linux")]
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    cmd
}

fn find_rigctld(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "rigctld.exe" } else { "rigctld" };
    let mut dirs: Vec<PathBuf> = resource_dir.map(|d| d.join("hamlib")).into_iter().collect();
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    // Apps started from the macOS Finder don't get the shell's PATH.
    dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from));
    dirs.into_iter().map(|d| d.join(exe)).find(|p| p.is_file())
}

fn not_found() -> String {
    if cfg!(target_os = "macos") {
        "Hamlib's rigctld isn't installed. Install it with Homebrew: brew install hamlib".into()
    } else if cfg!(windows) {
        "rigctld.exe is missing; reinstall OpenVKCL".into()
    } else {
        "Hamlib's rigctld isn't installed. Install it with: sudo apt install libhamlib-utils \
         (Fedora: sudo dnf install hamlib)"
            .into()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RigModel {
    pub id: u32,
    pub mfg: String,
    pub model: String,
}

/// The radios this Hamlib supports, from `rigctld -l`.
pub async fn models(resource_dir: Option<&Path>) -> Result<Vec<RigModel>, String> {
    let exe = find_rigctld(resource_dir).ok_or_else(not_found)?;
    let out = command(&exe)
        .arg("-l")
        .stdin(Stdio::null())
        .output()
        .await
        .map_err(|e| format!("Couldn't run {}: {e}", exe.display()))?;
    let models = parse_models(&String::from_utf8_lossy(&out.stdout));
    if models.is_empty() {
        return Err("rigctld didn't list any radio models".into());
    }
    Ok(models)
}

/// Parse the `rigctld -l` table, using the header to find the columns.
fn parse_models(text: &str) -> Vec<RigModel> {
    let mut lines = text.lines().skip_while(|l| !l.trim_start().starts_with("Rig #"));
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let (Some(mfg), Some(model), Some(version)) =
        (header.find("Mfg"), header.find("Model"), header.find("Version"))
    else {
        return Vec::new();
    };
    lines
        .filter_map(|l| {
            Some(RigModel {
                id: l.get(..mfg)?.trim().parse().ok()?,
                mfg: l.get(mfg..model)?.trim().to_string(),
                model: l.get(model..version)?.trim().to_string(),
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct SerialPort {
    /// What to pass to rigctld; on Linux a stable /dev/serial/by-id path where there is one.
    pub path: String,
    pub label: String,
}

pub fn serial_ports() -> Vec<SerialPort> {
    use serialport::SerialPortType;

    #[cfg(target_os = "linux")]
    let by_id: std::collections::HashMap<PathBuf, PathBuf> = std::fs::read_dir("/dev/serial/by-id")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| Some((e.path().canonicalize().ok()?, e.path())))
        .collect();

    let mut ports: Vec<SerialPort> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| {
            let usb = match &p.port_type {
                SerialPortType::UsbPort(u) => Some(u),
                _ => None,
            };
            // macOS lists every port twice; the cu. device is the one for outgoing connections.
            if cfg!(target_os = "macos") && p.port_name.starts_with("/dev/tty.") {
                return None;
            }
            // Linux has dozens of built-in ttyS ports, nearly always unused.
            if cfg!(target_os = "linux") && usb.is_none() && p.port_name.starts_with("/dev/ttyS") {
                return None;
            }
            let short = p.port_name.rsplit('/').next().unwrap_or(&p.port_name).to_string();
            let desc = usb
                .map(|u| {
                    [u.product.as_deref(), u.serial_number.as_deref()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .unwrap_or_default();
            #[cfg(target_os = "linux")]
            let path = by_id
                .get(Path::new(&p.port_name))
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or(p.port_name);
            #[cfg(not(target_os = "linux"))]
            let path = p.port_name;
            Some(SerialPort {
                path,
                label: if desc.is_empty() { short } else { format!("{short} – {desc}") },
            })
        })
        .collect();
    ports.sort_by(|a, b| a.label.cmp(&b.label));
    ports
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_model_list() {
        let text = "\
 Rig #  Mfg                    Model                   Version         Status      Macro
     1  Hamlib                 Dummy                   20211213.0      Stable      RIG_MODEL_DUMMY
     5  TRXManager             TRXManager 5.7.630+     20210613.0      Stable      RIG_MODEL_TRXMANAGER_RIG
  3081  Icom                   IC-9700                 20220128.6      Stable      RIG_MODEL_IC9700
";
        let models = parse_models(text);
        assert_eq!(models.len(), 3);
        assert_eq!(
            models[2],
            RigModel {
                id: 3081,
                mfg: "Icom".into(),
                model: "IC-9700".into()
            }
        );
        assert_eq!(models[1].model, "TRXManager 5.7.630+");
    }

    #[test]
    fn skips_backend_noise() {
        assert!(!is_message("initrigs4_icom: _init called"));
        assert!(is_message("rig_open: error = IO error"));
    }
}
