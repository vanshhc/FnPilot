use fnpilot_core::Result;
use fnpilot_core::session::Session;

pub const SAMPLE_RATE: usize = 16_000;
pub const MAX_SAMPLES: usize = SAMPLE_RATE * 60;

pub fn validate_audio(samples: &[f32]) -> Result<()> {
    if !(1_600..=MAX_SAMPLES).contains(&samples.len()) {
        return Err("Hold the talk key for 100 ms to 60 seconds.".into());
    }
    if samples.iter().any(|x| !x.is_finite() || x.abs() > 1.0) {
        return Err("Invalid microphone samples.".into());
    }
    let rms =
        (samples.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt();
    if rms <= 0.002 {
        return Err("No speech was detected.".into());
    }
    Ok(())
}

pub trait SpeechBackend {
    fn transcribe(&mut self, samples: &[f32], session: &Session, token: u64) -> Result<String>;
}

/// Session-owned transient capture. Late callbacks can never enter another capture.
#[derive(Default)]
pub struct Capture {
    generation: Option<u64>,
    samples: Vec<f32>,
    failed: bool,
}
impl Capture {
    pub fn start(&mut self, generation: u64) {
        self.cancel();
        self.generation = Some(generation);
    }
    pub fn append(&mut self, generation: u64, samples: &[f32]) {
        if self.generation != Some(generation) {
            return;
        }
        if self.samples.len() + samples.len() > MAX_SAMPLES {
            self.failed = true;
            return;
        }
        self.samples.extend_from_slice(samples);
    }
    pub fn finish(&mut self, generation: u64) -> Result<Vec<f32>> {
        if self.generation != Some(generation) {
            return Err("Recording was cancelled.".into());
        }
        self.generation = None;
        let samples = std::mem::take(&mut self.samples);
        if self.failed {
            return Err("Recording exceeded 60 seconds.".into());
        }
        validate_audio(&samples)?;
        Ok(samples)
    }
    pub fn cancel(&mut self) {
        self.generation = None;
        self.samples.clear();
        self.samples.shrink_to_fit();
        self.failed = false;
    }
}
