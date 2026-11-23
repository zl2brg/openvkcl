//! Voice keyer: play a recorded message to the radio's sound device with PTT
//! around it, and record messages from a microphone.

use crate::rig::Rig;
use rodio::Source;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

pub const STATUS_EVENT: &str = "keyer-status";

/// Keys in each message set, F1 to F12.
pub const KEYS: usize = 12;
/// The longest a recording may run, in case it's never stopped.
const MAX_RECORDING: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Message {
    pub label: String,
    /// A .wav file: a name in the voice folder, or a full path.
    pub file: String,
}

fn messages(labels: [&str; KEYS]) -> Vec<Message> {
    labels
        .iter()
        .map(|l| Message {
            label: (*l).to_string(),
            file: String::new(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct KeyerConfig {
    /// Sound output that feeds the radio; empty uses the system default.
    pub output: String,
    /// Microphone to record from; empty uses the system default.
    pub input: String,
    /// Key the radio through its connection; otherwise rely on its VOX.
    pub ptt: bool,
    /// Pause between keying and starting the audio, for the radio to switch over.
    pub ptt_delay_ms: u64,
    /// Pause between the end of one CQ and the next when repeating.
    pub repeat_ms: u64,
    pub run: Vec<Message>,
    pub sp: Vec<Message>,
}

impl Default for KeyerConfig {
    fn default() -> Self {
        // N1MM's default SSB keys.
        KeyerConfig {
            output: String::new(),
            input: String::new(),
            ptt: true,
            ptt_delay_ms: 150,
            repeat_ms: 3000,
            run: messages([
                "CQ", "Exch", "TU", "My call", "His call", "", "QRZ?", "Agn?", "Exch?", "", "", "",
            ]),
            sp: messages([
                "CQ", "S&P exch", "", "My call", "His call", "My call", "Rpt exch", "Agn?", "", "", "", "",
            ]),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct KeyerStatus {
    /// The file playing.
    pub playing: Option<String>,
    /// The file being recorded.
    pub recording: Option<String>,
    /// Set when a message finishes on its own (not stopped), for CQ repeat.
    pub finished: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Devices {
    pub outputs: Vec<String>,
    pub inputs: Vec<String>,
}

fn device_name(d: &rodio::cpal::Device) -> String {
    use rodio::cpal::traits::DeviceTrait;
    d.description().map_or_else(|_| "unknown".to_string(), |d| d.name().to_string())
}

fn output_devices() -> Vec<rodio::cpal::Device> {
    use rodio::cpal::traits::HostTrait;
    rodio::cpal::default_host()
        .output_devices()
        .map(|d| d.collect())
        .unwrap_or_default()
}

pub fn devices() -> Devices {
    let names = |v: Vec<String>| {
        let mut v = v;
        v.dedup();
        v
    };
    Devices {
        outputs: names(output_devices().iter().map(device_name).collect()),
        inputs: names(
            rodio::microphone::available_inputs()
                .map(|i| i.into_iter().map(|d| d.to_string()).collect())
                .unwrap_or_default(),
        ),
    }
}

/// A message's file: as given if it's a full path, else in the voice folder.
pub fn resolve(voice_dir: &Path, file: &str) -> PathBuf {
    let p = Path::new(file);
    if p.is_absolute() { p.to_path_buf() } else { voice_dir.join(p) }
}

/// Play `path` on the named output (or the default) until it ends or `stop` is set.
fn play_blocking(path: &Path, output: &str, stop: &AtomicBool) -> Result<(), String> {
    use rodio::DeviceSinkBuilder;
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut sink = match output_devices().into_iter().find(|d| !output.is_empty() && device_name(d) == output) {
        Some(device) => DeviceSinkBuilder::from_device(device).and_then(|b| b.open_stream()),
        None if output.is_empty() => DeviceSinkBuilder::open_default_sink(),
        None => return Err(format!("Sound output \"{output}\" isn't connected")),
    }
    .map_err(|e| format!("Can't open the sound output: {e}"))?;
    sink.log_on_drop(false);
    let player = rodio::play(sink.mixer(), std::io::BufReader::new(file))
        .map_err(|e| format!("Can't play {}: {e}", path.display()))?;
    while !player.empty() && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(20));
    }
    player.stop();
    Ok(())
}

/// Record from the named input (or the default) into a mono 16-bit .wav at `path`.
fn record_blocking(path: &Path, input: &str, stop: &AtomicBool) -> Result<(), String> {
    use rodio::microphone::{MicrophoneBuilder, available_inputs};
    let builder = MicrophoneBuilder::new();
    let builder = match available_inputs()
        .ok()
        .and_then(|i| i.into_iter().find(|d| !input.is_empty() && d.to_string() == input))
    {
        Some(device) => builder.device(device),
        None if input.is_empty() => builder.default_device(),
        None => return Err(format!("Microphone \"{input}\" isn't connected")),
    }
    .map_err(|e| e.to_string())?;
    let mut mic = builder
        .default_config()
        .map_err(|e| e.to_string())?
        .open_stream()
        .map_err(|e| e.to_string())?;
    let channels = mic.channels().get() as usize;
    let rate = mic.sample_rate().get();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wav = hound::WavWriter::create(path, spec).map_err(|e| format!("{}: {e}", path.display()))?;
    let started = Instant::now();
    let mut frame = Vec::with_capacity(channels);
    while !stop.load(Ordering::Relaxed) && started.elapsed() < MAX_RECORDING {
        let Some(sample) = mic.next() else { break };
        frame.push(sample);
        if frame.len() == channels {
            // Mix to mono, which is all a voice message needs.
            let mono = frame.iter().sum::<f32>() / channels as f32;
            wav.write_sample((mono.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                .map_err(|e| e.to_string())?;
            frame.clear();
        }
    }
    wav.finalize().map_err(|e| e.to_string())
}

#[derive(Default)]
pub struct Keyer {
    /// Stops whatever is playing or recording.
    stop: Mutex<Option<Arc<AtomicBool>>>,
    status: Arc<Mutex<KeyerStatus>>,
}

fn publish(app: &AppHandle, shared: &Mutex<KeyerStatus>, status: KeyerStatus) {
    *shared.lock().unwrap() = status.clone();
    let _ = app.emit(STATUS_EVENT, status);
}

impl Keyer {
    pub fn status(&self) -> KeyerStatus {
        self.status.lock().unwrap().clone()
    }

    /// Stop playing or recording; a message stopped this way unkeys the radio.
    pub fn stop(&self) {
        if let Some(flag) = self.stop.lock().unwrap().take() {
            flag.store(true, Ordering::Relaxed);
        }
    }

    fn begin(&self) -> Arc<AtomicBool> {
        self.stop();
        let flag = Arc::new(AtomicBool::new(false));
        *self.stop.lock().unwrap() = Some(flag.clone());
        flag
    }

    /// Key the radio, play `path`, then unkey it.
    pub fn play(&self, app: AppHandle, rig: Arc<Rig>, cfg: KeyerConfig, path: PathBuf) {
        let stop = self.begin();
        let shared = self.status.clone();
        let name = path.display().to_string();
        tauri::async_runtime::spawn(async move {
            if !path.is_file() {
                let error = Some(format!("No recording at {name}: record it with Ctrl+Shift+F-key"));
                publish(&app, &shared, KeyerStatus { error, ..Default::default() });
                return;
            }
            publish(&app, &shared, KeyerStatus { playing: Some(name.clone()), ..Default::default() });
            let keyed = if cfg.ptt { rig.ptt(true).await } else { Ok(false) };
            let result = match keyed {
                Err(e) => Err(format!("PTT: {e}")),
                Ok(keyed) => {
                    if keyed {
                        tokio::time::sleep(Duration::from_millis(cfg.ptt_delay_ms)).await;
                    }
                    let (output, stop2) = (cfg.output.clone(), stop.clone());
                    let played = tauri::async_runtime::spawn_blocking(move || play_blocking(&path, &output, &stop2))
                        .await
                        .unwrap_or_else(|e| Err(e.to_string()));
                    if keyed && let Err(e) = rig.ptt(false).await {
                        // Make sure the radio isn't left transmitting.
                        let _ = rig.ptt(false).await;
                        played.and(Err(format!("PTT off: {e}")))
                    } else {
                        played
                    }
                }
            };
            let stopped = stop.load(Ordering::Relaxed);
            publish(
                &app,
                &shared,
                KeyerStatus {
                    finished: (result.is_ok() && !stopped).then_some(name),
                    error: result.err(),
                    ..Default::default()
                },
            );
        });
    }

    /// Record from the microphone into `path` until stopped.
    pub fn record(&self, app: AppHandle, cfg: KeyerConfig, path: PathBuf) {
        let stop = self.begin();
        let shared = self.status.clone();
        let name = path.display().to_string();
        publish(&app, &shared, KeyerStatus { recording: Some(name), ..Default::default() });
        tauri::async_runtime::spawn(async move {
            let result = tauri::async_runtime::spawn_blocking(move || record_blocking(&path, &cfg.input, &stop))
                .await
                .unwrap_or_else(|e| Err(e.to_string()));
            publish(&app, &shared, KeyerStatus { error: result.err(), ..Default::default() });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_paths() {
        let c = KeyerConfig::default();
        assert_eq!((c.run.len(), c.sp.len()), (KEYS, KEYS));
        assert_eq!(c.run[0].label, "CQ");
        let dir = Path::new("/voice");
        assert_eq!(resolve(dir, "cq.wav"), Path::new("/voice/cq.wav"));
        let abs = if cfg!(windows) { "C:\\msgs\\cq.wav" } else { "/msgs/cq.wav" };
        assert_eq!(resolve(dir, abs), Path::new(abs));
        // Settings from before the keyer get its defaults.
        let s: KeyerConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(s, KeyerConfig::default());
    }
}
