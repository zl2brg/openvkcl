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
/// The microphone's level while recording, 0 to 1, about ten times a second.
pub const LEVEL_EVENT: &str = "keyer-level";

/// A recording quieter than this all through (about -50 dBFS) is taken to be
/// silence: on a Mac, what the app gets without microphone access.
const SILENT: f32 = 0.003;
/// How often the level is reported while recording.
const LEVEL_EVERY: Duration = Duration::from_millis(100);

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
/// A recording decoded to mono samples at `rate` Hz.
pub fn load_mono(path: &Path, rate: u32) -> Result<Vec<f32>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let decoder = rodio::Decoder::try_from(file).map_err(|e| format!("Can't read {}: {e}", path.display()))?;
    let channels = decoder.channels().get() as usize;
    let from = decoder.sample_rate().get();
    let samples: Vec<f32> = decoder.collect();
    let mono: Vec<f32> = samples
        .chunks(channels)
        .map(|frame| frame.iter().sum::<f32>() / frame.len() as f32)
        .collect();
    Ok(resample(&mono, from, rate))
}

/// Change the sample rate by linear interpolation: plenty for a voice message.
pub fn resample(input: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || input.is_empty() {
        return input.to_vec();
    }
    let step = from as f64 / to as f64;
    let len = ((input.len() as f64) / step).floor() as usize;
    (0..len)
        .map(|i| {
            let pos = i as f64 * step;
            let k = pos as usize;
            let frac = (pos - k as f64) as f32;
            let a = input[k];
            let b = *input.get(k + 1).unwrap_or(&a);
            a + (b - a) * frac
        })
        .collect()
}

/// Play `path` on the named output (or the default) until it ends or `stop` is set.
fn play_blocking(path: &Path, output: &str, stop: &AtomicBool) -> Result<(), String> {
    use rodio::DeviceSinkBuilder;
    let mut sink = match output_devices().into_iter().find(|d| !output.is_empty() && device_name(d) == output) {
        Some(device) => DeviceSinkBuilder::from_device(device).and_then(|b| b.open_stream()),
        None if output.is_empty() => DeviceSinkBuilder::open_default_sink(),
        None => return Err(format!("Sound output \"{output}\" isn't connected")),
    }
    .map_err(|e| format!("Can't open the sound output: {e}"))?;
    sink.log_on_drop(false);
    // The mixer plays at the device's own rate and doesn't convert, so convert
    // the message to it first; otherwise a 44.1 kHz recording on a 24 kHz
    // device plays slow and low.
    let (channels, rate) = (sink.config().channel_count(), sink.config().sample_rate());
    let mono = load_mono(path, rate.get())?;
    let frames: Vec<f32> = mono
        .iter()
        .flat_map(|&s| std::iter::repeat_n(s, channels.get() as usize))
        .collect();
    let player = rodio::Player::connect_new(sink.mixer());
    player.append(rodio::buffer::SamplesBuffer::new(channels, rate, frames));
    while !player.empty() && !stop.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(20));
    }
    player.stop();
    Ok(())
}

/// Record from the named input (or the default) into a mono 16-bit .wav at
/// `path`, reporting the level to `level` as it goes. A silent recording is
/// deleted and reported as an error.
fn record_blocking(path: &Path, input: &str, stop: &AtomicBool, level: impl Fn(f32)) -> Result<(), String> {
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
    let (mut peak, mut window_peak, mut reported) = (0f32, 0f32, Instant::now());
    while !stop.load(Ordering::Relaxed) && started.elapsed() < MAX_RECORDING {
        let Some(sample) = mic.next() else { break };
        frame.push(sample);
        if frame.len() == channels {
            // Mix to mono, which is all a voice message needs.
            let mono = frame.iter().sum::<f32>() / channels as f32;
            wav.write_sample((mono.clamp(-1.0, 1.0) * i16::MAX as f32) as i16)
                .map_err(|e| e.to_string())?;
            frame.clear();
            window_peak = window_peak.max(mono.abs());
            if reported.elapsed() >= LEVEL_EVERY {
                level(window_peak.min(1.0));
                peak = peak.max(window_peak);
                window_peak = 0.0;
                reported = Instant::now();
            }
        }
    }
    level(0.0);
    wav.finalize().map_err(|e| e.to_string())?;
    if peak.max(window_peak) < SILENT {
        let _ = std::fs::remove_file(path);
        return Err(if cfg!(target_os = "macos") {
            "The recording was silent. Allow OpenVKCL to use the microphone in System Settings → \
             Privacy & Security → Microphone, or choose another microphone under Voice keyer…, then record again"
                .into()
        } else {
            "The recording was silent: check the microphone chosen under Voice keyer… and its level, then record again"
                .into()
        });
    }
    Ok(())
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
            let result = if cfg.ptt && rig.carries_audio() {
                // A TCI server (AetherSDR, ExpertSDR, Thetis) takes the message as
                // the transmit audio itself, keyed up by the same connection.
                let decoded = tauri::async_runtime::spawn_blocking(move || load_mono(&path, crate::rig::tci::TX_RATE))
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()));
                match decoded {
                    Ok(audio) => rig.transmit(audio, stop.clone()).await,
                    Err(e) => Err(e),
                }
            } else {
                play_with_ptt(&rig, &cfg, path, stop.clone()).await
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
}

/// Key the radio (unless relying on its VOX), play the message on the sound
/// output, and unkey.
async fn play_with_ptt(rig: &Rig, cfg: &KeyerConfig, path: PathBuf, stop: Arc<AtomicBool>) -> Result<(), String> {
    let keyed = if cfg.ptt {
        rig.ptt(true).await.map_err(|e| format!("PTT: {e}"))?
    } else {
        false
    };
    if keyed {
        tokio::time::sleep(Duration::from_millis(cfg.ptt_delay_ms)).await;
    }
    let output = cfg.output.clone();
    let played = tauri::async_runtime::spawn_blocking(move || play_blocking(&path, &output, &stop))
        .await
        .unwrap_or_else(|e| Err(e.to_string()));
    if keyed && let Err(e) = rig.ptt(false).await {
        // Make sure the radio isn't left transmitting.
        let _ = rig.ptt(false).await;
        return played.and(Err(format!("PTT off: {e}")));
    }
    played
}

impl Keyer {

    /// Record from the microphone into `path` until stopped.
    pub fn record(&self, app: AppHandle, cfg: KeyerConfig, path: PathBuf) {
        let stop = self.begin();
        let shared = self.status.clone();
        let name = path.display().to_string();
        publish(&app, &shared, KeyerStatus { recording: Some(name), ..Default::default() });
        tauri::async_runtime::spawn(async move {
            let meter = app.clone();
            let level = move |l: f32| {
                let _ = meter.emit(LEVEL_EVENT, l);
            };
            let result = tauri::async_runtime::spawn_blocking(move || record_blocking(&path, &cfg.input, &stop, level))
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

    #[test]
    fn resampling() {
        // 44.1 kHz to 48 kHz keeps the length in time and the shape.
        let ramp: Vec<f32> = (0..441).map(|i| i as f32 / 441.0).collect();
        let out = resample(&ramp, 44_100, 48_000);
        assert_eq!(out.len(), 480);
        assert!((out[240] - 0.5).abs() < 0.01, "{}", out[240]);
        assert_eq!(resample(&ramp, 48_000, 48_000), ramp);
        assert_eq!(resample(&ramp, 44_100, 24_000).len(), 240);
    }

    #[test]
    fn decoding_a_recording() {
        let path = std::env::temp_dir().join(format!("openvkcl-load-{}.wav", std::process::id()));
        let spec = hound::WavSpec { channels: 2, sample_rate: 24_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..2400 {
            w.write_sample(i16::MAX / 2).unwrap();
            w.write_sample(i16::MAX / 2).unwrap();
        }
        w.finalize().unwrap();
        // 0.1 s of stereo at 24 kHz becomes 0.1 s of mono at 48 kHz.
        let mono = load_mono(&path, 48_000).unwrap();
        assert_eq!(mono.len(), 4800);
        assert!((mono[100] - 0.5).abs() < 0.01);
        std::fs::remove_file(path).ok();
    }
}
