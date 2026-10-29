//! Bounded streaming conversion to 16 kHz mono, including anti-alias filtering.
use crate::backend::SAMPLE_RATE;
use fnpilot_core::Result;
use std::collections::VecDeque;

pub struct Resampler {
    rate: u32,
    history: VecDeque<f32>,
    base: i64,
    input: u64,
    output: u64,
}
const RADIUS: i64 = 16;
impl Resampler {
    pub fn new(rate: u32) -> Result<Self> {
        if !(8000..=192000).contains(&rate) {
            return Err("Unsupported microphone sample rate.".into());
        }
        Ok(Self {
            rate,
            history: VecDeque::new(),
            base: 0,
            input: 0,
            output: 0,
        })
    }
    pub fn process(&mut self, samples: &[f32], channels: usize) -> Vec<f32> {
        if channels == 0 {
            return Vec::new();
        }
        let mut output = Vec::new();
        for frame in samples.chunks_exact(channels) {
            let mono = frame.iter().sum::<f32>() / channels as f32;
            self.push(mono, &mut output);
        }
        output
    }
    fn push(&mut self, sample: f32, output: &mut Vec<f32>) {
        self.input += 1;
        if self.rate == SAMPLE_RATE as u32 {
            self.output += 1;
            output.push(sample);
            return;
        }
        self.history.push_back(sample);
        self.drain(output, false);
    }
    fn drain(&mut self, output: &mut Vec<f32>, flush: bool) {
        let total = self.input * SAMPLE_RATE as u64 / self.rate as u64;
        while self.output < total {
            let position = self.output as f64 * self.rate as f64 / SAMPLE_RATE as f64;
            let center = position.floor() as i64;
            if !flush && center + RADIUS >= self.input as i64 {
                break;
            }
            let cutoff = (SAMPLE_RATE as f64 / self.rate as f64).min(1.) * 0.45;
            let (mut sum, mut weight) = (0., 0.);
            for index in center - RADIUS + 1..=center + RADIUS {
                let x = index as f64 - position;
                if x.abs() >= RADIUS as f64 {
                    continue;
                }
                let sinc = if x.abs() < 1e-10 {
                    2. * cutoff
                } else {
                    (2. * std::f64::consts::PI * cutoff * x).sin() / (std::f64::consts::PI * x)
                };
                let coefficient =
                    sinc * (0.5 + 0.5 * (std::f64::consts::PI * x / RADIUS as f64).cos());
                let sample = if index < self.base {
                    0.
                } else {
                    self.history
                        .get((index - self.base) as usize)
                        .copied()
                        .unwrap_or(0.)
                };
                sum += sample as f64 * coefficient;
                weight += coefficient;
            }
            output.push((sum / weight) as f32);
            self.output += 1;
            let keep = (self.output as f64 * self.rate as f64 / SAMPLE_RATE as f64).floor() as i64
                - RADIUS;
            while self.base < keep && !self.history.is_empty() {
                self.history.pop_front();
                self.base += 1;
            }
        }
    }
    pub fn finish(&mut self) -> Vec<f32> {
        let mut output = Vec::new();
        if self.rate != SAMPLE_RATE as u32 {
            self.drain(&mut output, true);
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_rates_and_stereo_mix_keep_the_exact_duration() {
        for rate in [8000, 16000, 44100, 48000, 96000] {
            let input = (0..rate / 10)
                .flat_map(|_| [0.25f32, 0.75])
                .collect::<Vec<_>>();
            let mut converter = Resampler::new(rate).unwrap();
            let mut result = Vec::new();
            for chunk in input.chunks(138) {
                result.extend(converter.process(chunk, 2));
            }
            result.extend(converter.finish());
            assert_eq!(result.len(), 1600, "rate {rate}");
            assert!(
                result[64..result.len() - 64]
                    .iter()
                    .all(|v| (*v - 0.5).abs() < 1e-4)
            );
        }
    }
    #[test]
    fn downsampling_rejects_out_of_band_noise_without_removing_speech_band() {
        let rms = |frequency: f64| {
            let input = (0..4800)
                .map(|i| {
                    (2. * std::f64::consts::PI * frequency * i as f64 / 48000.).sin() as f32 * 0.5
                })
                .collect::<Vec<_>>();
            let mut converter = Resampler::new(48000).unwrap();
            let mut result = converter.process(&input, 1);
            result.extend(converter.finish());
            (result[64..result.len() - 64]
                .iter()
                .map(|v| (*v as f64).powi(2))
                .sum::<f64>()
                / (result.len() - 128) as f64)
                .sqrt()
        };
        assert!(rms(1000.) > 0.3);
        assert!(rms(12000.) < 0.01);
    }
}
