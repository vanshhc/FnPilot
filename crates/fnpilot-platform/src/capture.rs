//! Shared transient capture. Platform listeners control only epoch and physical hold state.
#[cfg(target_os = "macos")]
use crate::macos::{current_epoch, held};
#[cfg(target_os = "windows")]
use crate::windows::{current_epoch, held};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fnpilot_core::Result;
use fnpilot_speech::{backend::Capture, resample::Resampler};
use std::sync::{Arc, Mutex};

type Job = (
    String,
    u64,
    std::sync::mpsc::Sender<Result<serde_json::Value>>,
);
fn worker() -> &'static std::sync::mpsc::Sender<Job> {
    static WORKER: std::sync::OnceLock<std::sync::mpsc::Sender<Job>> = std::sync::OnceLock::new();
    WORKER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        std::thread::spawn(move || {
            let mut recorder = Recorder::default();
            for (operation, epoch, reply) in rx {
                if current_epoch() != epoch {
                    let _ = reply.send(Err("Stale microphone operation.".into()));
                    continue;
                }
                let result = match operation.as_str() {
                    "startCapture" => recorder.start(epoch).map(|_| serde_json::json!(true)),
                    "stopCapture" => recorder.stop().map(|samples| serde_json::json!(samples)),
                    "endCapture" => {
                        recorder.end_capture();
                        Ok(serde_json::json!(true))
                    }
                    _ => {
                        recorder.cancel();
                        Ok(serde_json::json!(true))
                    }
                };
                let _ = reply.send(result);
            }
        });
        tx
    })
}
// Event-tap callbacks must never wait for device shutdown on the audio worker.
#[cfg(target_os = "macos")]
pub fn notify(operation: &str) {
    let (reply, _) = std::sync::mpsc::channel();
    let _ = worker().send((operation.into(), current_epoch(), reply));
}
pub fn request(operation: &str) -> Result<serde_json::Value> {
    let (tx, rx) = std::sync::mpsc::channel();
    worker()
        .send((operation.into(), current_epoch(), tx))
        .map_err(|_| "Microphone worker stopped.")?;
    rx.recv_timeout(std::time::Duration::from_secs(3))
        .map_err(|_| "Microphone operation timed out.".to_string())?
}

pub struct Recorder {
    stream: Option<cpal::Stream>,
    capture: Arc<Mutex<Capture>>,
    token: u64,
    resampler: Arc<Mutex<Option<Resampler>>>,
}
impl Default for Recorder {
    fn default() -> Self {
        Self {
            stream: None,
            capture: Arc::new(Mutex::new(Capture::default())),
            token: 0,
            resampler: Arc::new(Mutex::new(None)),
        }
    }
}
impl Recorder {
    pub fn start(&mut self, token: u64) -> Result<()> {
        self.cancel();
        if !held() {
            return Err("The talk key is no longer held.".into());
        }
        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or("No microphone is available.")?;
        let supported = device
            .default_input_config()
            .map_err(|_| "Microphone permission or format is unavailable.")?;
        let config: cpal::StreamConfig = supported.clone().into();
        let channels = config.channels as usize;
        let rate = config.sample_rate.0;
        if channels == 0 {
            return Err("No microphone channels are available.".into());
        }
        *self.resampler.lock().unwrap() = Some(Resampler::new(rate)?);
        self.capture.lock().unwrap().start(token);
        self.token = token;
        let build = |format: cpal::SampleFormat| -> Result<cpal::Stream> {
            let capture = self.capture.clone();
            let failure = self.capture.clone();
            let resampler = self.resampler.clone();
            let process = move |data: Vec<f32>| {
                if current_epoch() != token || !held() {
                    return;
                }
                let mut converter = resampler.lock().unwrap();
                let Some(converter) = converter.as_mut() else {
                    return;
                };
                let mono = converter
                    .process(&data, channels)
                    .into_iter()
                    .map(|sample| sample.clamp(-1., 1.))
                    .collect::<Vec<_>>();
                capture.lock().unwrap().append(token, &mono);
            };
            let error = move |_| {
                failure.lock().unwrap().cancel();
            };
            match format {
                cpal::SampleFormat::F32 => device.build_input_stream(
                    &config,
                    move |data: &[f32], _| process(data.to_vec()),
                    error,
                    None,
                ),
                cpal::SampleFormat::I16 => device.build_input_stream(
                    &config,
                    move |data: &[i16], _| {
                        process(data.iter().map(|v| *v as f32 / 32768.0).collect())
                    },
                    error,
                    None,
                ),
                cpal::SampleFormat::U16 => device.build_input_stream(
                    &config,
                    move |data: &[u16], _| {
                        process(
                            data.iter()
                                .map(|v| (*v as f32 - 32768.0) / 32768.0)
                                .collect(),
                        )
                    },
                    error,
                    None,
                ),
                _ => return Err("The microphone format is unsupported.".into()),
            }
            .map_err(|_| "Could not start native microphone capture.".to_string())
        };
        let stream = build(supported.sample_format())?;
        stream
            .play()
            .map_err(|_| "Could not activate microphone capture.")?;
        if current_epoch() != token || !held() {
            self.cancel();
            return Err("Recording start was cancelled.".into());
        }
        self.stream = Some(stream);
        Ok(())
    }
    fn end_capture(&mut self) {
        self.stream.take();
        if let Some(mut converter) = self.resampler.lock().unwrap().take() {
            let tail = converter
                .finish()
                .into_iter()
                .map(|v| v.clamp(-1., 1.))
                .collect::<Vec<_>>();
            self.capture.lock().unwrap().append(self.token, &tail);
        }
    }
    pub fn stop(&mut self) -> Result<Vec<f32>> {
        self.end_capture();
        self.capture.lock().unwrap().finish(self.token)
    }
    pub fn cancel(&mut self) {
        self.stream.take();
        self.resampler.lock().unwrap().take();
        self.capture.lock().unwrap().cancel();
    }
}
