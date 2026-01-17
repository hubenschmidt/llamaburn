use std::cmp::Ord;
use std::fmt;

use fundsp::hacker::*;

use super::{AudioEffect, EffectParam};

/// Simple gain/volume control
#[derive(Debug)]
pub struct GainEffect {
    gain_db: f32,
    gain_linear: f32,
    bypassed: bool,
}

impl GainEffect {
    pub fn new(gain_db: f32) -> Self {
        Self {
            gain_db,
            gain_linear: db_amp(gain_db) as f32,
            bypassed: false,
        }
    }
}

impl AudioEffect for GainEffect {
    fn name(&self) -> &str {
        "Gain"
    }

    fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            *sample *= self.gain_linear;
        }
    }

    fn set_param(&mut self, name: &str, value: f32) {
        if name == "gain" {
            self.gain_db = value;
            self.gain_linear = db_amp(value) as f32;
        }
    }

    fn get_params(&self) -> Vec<EffectParam> {
        vec![EffectParam::new("gain", self.gain_db, -60.0, 24.0, "dB")]
    }

    fn set_bypass(&mut self, bypass: bool) {
        self.bypassed = bypass;
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }
}

/// High-pass filter - removes low frequencies
pub struct HighPassEffect {
    cutoff_hz: f32,
    filter: An<FixedSvf<f64, HighpassMode<f64>>>,
    bypassed: bool,
}

impl HighPassEffect {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let mut filter = highpass_hz(cutoff_hz, 0.707);
        filter.set_sample_rate(sample_rate as f64);
        Self {
            cutoff_hz,
            filter,
            bypassed: false,
        }
    }
}

impl fmt::Debug for HighPassEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HighPassEffect")
            .field("cutoff_hz", &self.cutoff_hz)
            .field("bypassed", &self.bypassed)
            .finish()
    }
}

impl AudioEffect for HighPassEffect {
    fn name(&self) -> &str {
        "High Pass"
    }

    fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            let input = Frame::from([*sample]);
            let output = self.filter.tick(&input);
            *sample = output[0];
        }
    }

    fn set_param(&mut self, name: &str, value: f32) {
        if name == "cutoff" {
            self.cutoff_hz = value;
            self.filter.set(Setting::center(value));
        }
    }

    fn get_params(&self) -> Vec<EffectParam> {
        vec![EffectParam::new("cutoff", self.cutoff_hz, 20.0, 2000.0, "Hz")]
    }

    fn set_bypass(&mut self, bypass: bool) {
        self.bypassed = bypass;
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.filter.set_sample_rate(sample_rate as f64);
    }
}

/// Low-pass filter - removes high frequencies
pub struct LowPassEffect {
    cutoff_hz: f32,
    filter: An<FixedSvf<f64, LowpassMode<f64>>>,
    bypassed: bool,
}

impl LowPassEffect {
    pub fn new(cutoff_hz: f32, sample_rate: f32) -> Self {
        let mut filter = lowpass_hz(cutoff_hz, 0.707);
        filter.set_sample_rate(sample_rate as f64);
        Self {
            cutoff_hz,
            filter,
            bypassed: false,
        }
    }
}

impl fmt::Debug for LowPassEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LowPassEffect")
            .field("cutoff_hz", &self.cutoff_hz)
            .field("bypassed", &self.bypassed)
            .finish()
    }
}

impl AudioEffect for LowPassEffect {
    fn name(&self) -> &str {
        "Low Pass"
    }

    fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            let input = Frame::from([*sample]);
            let output = self.filter.tick(&input);
            *sample = output[0];
        }
    }

    fn set_param(&mut self, name: &str, value: f32) {
        if name == "cutoff" {
            self.cutoff_hz = value;
            self.filter.set(Setting::center(value));
        }
    }

    fn get_params(&self) -> Vec<EffectParam> {
        vec![EffectParam::new("cutoff", self.cutoff_hz, 200.0, 20000.0, "Hz")]
    }

    fn set_bypass(&mut self, bypass: bool) {
        self.bypassed = bypass;
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        self.filter.set_sample_rate(sample_rate as f64);
    }
}

/// Simple compressor using fundsp limiter
pub struct CompressorEffect {
    threshold_db: f32,
    attack_ms: f32,
    release_ms: f32,
    limiter: An<Limiter<U1>>,
    bypassed: bool,
}

impl CompressorEffect {
    pub fn new(threshold_db: f32, attack_ms: f32, release_ms: f32) -> Self {
        let attack_s = attack_ms / 1000.0;
        let release_s = release_ms / 1000.0;
        let lim = limiter(attack_s, release_s);
        Self {
            threshold_db,
            attack_ms,
            release_ms,
            limiter: lim,
            bypassed: false,
        }
    }
}

impl fmt::Debug for CompressorEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompressorEffect")
            .field("threshold_db", &self.threshold_db)
            .field("attack_ms", &self.attack_ms)
            .field("release_ms", &self.release_ms)
            .field("bypassed", &self.bypassed)
            .finish()
    }
}

impl AudioEffect for CompressorEffect {
    fn name(&self) -> &str {
        "Compressor"
    }

    fn process(&mut self, samples: &mut [f32]) {
        let threshold_linear = db_amp(self.threshold_db) as f32;
        for sample in samples.iter_mut() {
            // Scale to threshold, apply limiter, scale back
            let scaled = *sample / threshold_linear;
            let input = Frame::from([scaled]);
            let output = self.limiter.tick(&input);
            *sample = output[0] * threshold_linear;
        }
    }

    fn set_param(&mut self, name: &str, value: f32) {
        match name {
            "threshold" => self.threshold_db = value,
            "attack" => {
                self.attack_ms = value;
                let attack_s = value / 1000.0;
                let release_s = self.release_ms / 1000.0;
                self.limiter = limiter(attack_s, release_s);
            }
            "release" => {
                self.release_ms = value;
                let attack_s = self.attack_ms / 1000.0;
                let release_s = value / 1000.0;
                self.limiter = limiter(attack_s, release_s);
            }
            _ => {}
        }
    }

    fn get_params(&self) -> Vec<EffectParam> {
        vec![
            EffectParam::new("threshold", self.threshold_db, -60.0, 0.0, "dB"),
            EffectParam::new("attack", self.attack_ms, 0.1, 100.0, "ms"),
            EffectParam::new("release", self.release_ms, 10.0, 1000.0, "ms"),
        ]
    }

    fn set_bypass(&mut self, bypass: bool) {
        self.bypassed = bypass;
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }
}

/// Simple delay effect with feedback - dynamic delay time
pub struct DelayEffect {
    delay_ms: f32,
    feedback: f32,
    mix: f32,
    buffer: Vec<f32>,
    write_pos: usize,
    sample_rate: f32,
    max_delay_ms: f32,
    bypassed: bool,
}

impl DelayEffect {
    pub fn new(delay_ms: f32, feedback: f32, mix: f32, sample_rate: f32) -> Self {
        let max_delay_ms = 2000.0; // 2 second max buffer
        let max_samples = (max_delay_ms * sample_rate / 1000.0) as usize;
        Self {
            delay_ms: delay_ms.clamp(1.0, max_delay_ms),
            feedback: feedback.clamp(0.0, 1.0),
            mix: mix.clamp(0.0, 1.0),
            buffer: vec![0.0; max_samples],
            write_pos: 0,
            sample_rate,
            max_delay_ms,
            bypassed: false,
        }
    }

}

impl fmt::Debug for DelayEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DelayEffect")
            .field("delay_ms", &self.delay_ms)
            .field("feedback", &self.feedback)
            .field("mix", &self.mix)
            .field("bypassed", &self.bypassed)
            .finish()
    }
}

impl AudioEffect for DelayEffect {
    fn name(&self) -> &str {
        "Delay"
    }

    fn process(&mut self, samples: &mut [f32]) {
        let delay_samples = (self.delay_ms * self.sample_rate / 1000.0) as usize;
        let delay_samples = delay_samples.clamp(1, self.buffer.len() - 1);

        for sample in samples.iter_mut() {
            // Read from delayed position
            let read_pos = (self.write_pos + self.buffer.len() - delay_samples) % self.buffer.len();
            let delayed = self.buffer[read_pos];

            // Write input + feedback to current position
            self.buffer[self.write_pos] = *sample + delayed * self.feedback;
            self.write_pos = (self.write_pos + 1) % self.buffer.len();

            // Mix dry and wet
            *sample = *sample * (1.0 - self.mix) + delayed * self.mix;
        }
    }

    fn set_param(&mut self, name: &str, value: f32) {
        match name {
            "delay" => self.delay_ms = value.clamp(1.0, self.max_delay_ms),
            "feedback" => self.feedback = value.clamp(0.0, 1.0),
            "mix" => self.mix = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_params(&self) -> Vec<EffectParam> {
        vec![
            EffectParam::new("delay", self.delay_ms, 1.0, self.max_delay_ms, "ms"),
            EffectParam::new("feedback", self.feedback, 0.0, 1.0, ""),
            EffectParam::new("mix", self.mix, 0.0, 1.0, ""),
        ]
    }

    fn set_bypass(&mut self, bypass: bool) {
        self.bypassed = bypass;
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        let old_rate = self.sample_rate;
        if (sample_rate - old_rate).abs() < 1.0 {
            return;
        }
        self.sample_rate = sample_rate;
        let max_samples = (self.max_delay_ms * sample_rate / 1000.0) as usize;
        self.buffer = vec![0.0; max_samples];
        self.write_pos = 0;
    }
}

/// Simple reverb using multiple delay lines (Schroeder reverb)
pub struct ReverbEffect {
    room_size: f32,
    damping: f32,
    mix: f32,
    delays: Vec<Vec<f32>>,
    positions: Vec<usize>,
    sample_rate: f32,
    bypassed: bool,
}

impl ReverbEffect {
    pub fn new(room_size: f32, damping: f32, mix: f32, sample_rate: f32) -> Self {
        let mut effect = Self {
            room_size: room_size.clamp(0.0, 1.0),
            damping: damping.clamp(0.0, 1.0),
            mix: mix.clamp(0.0, 1.0),
            delays: Vec::new(),
            positions: Vec::new(),
            sample_rate,
            bypassed: false,
        };
        effect.rebuild_delays();
        effect
    }

    fn rebuild_delays(&mut self) {
        // Prime-number based delay times for diffusion
        let base_delay = self.room_size * 50.0 + 10.0; // 10-60ms base
        let delay_times_ms = [
            base_delay * 1.0,
            base_delay * 1.13,
            base_delay * 1.27,
            base_delay * 1.41,
        ];

        self.delays = delay_times_ms
            .iter()
            .map(|&ms| {
                let samples = (ms * self.sample_rate / 1000.0) as usize;
                vec![0.0; Ord::max(samples, 1)]
            })
            .collect();
        self.positions = vec![0; self.delays.len()];
    }
}

impl fmt::Debug for ReverbEffect {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReverbEffect")
            .field("room_size", &self.room_size)
            .field("damping", &self.damping)
            .field("mix", &self.mix)
            .field("bypassed", &self.bypassed)
            .finish()
    }
}

impl AudioEffect for ReverbEffect {
    fn name(&self) -> &str {
        "Reverb"
    }

    fn process(&mut self, samples: &mut [f32]) {
        let feedback = 0.7 * (1.0 - self.damping * 0.4);

        for sample in samples.iter_mut() {
            let dry = *sample;
            let mut wet = 0.0;

            for (i, delay_buf) in self.delays.iter_mut().enumerate() {
                let pos = self.positions[i];
                let delayed = delay_buf[pos];
                wet += delayed;
                delay_buf[pos] = dry + delayed * feedback;
                self.positions[i] = (pos + 1) % delay_buf.len();
            }

            wet /= self.delays.len() as f32;
            *sample = dry * (1.0 - self.mix) + wet * self.mix;
        }
    }

    fn set_param(&mut self, name: &str, value: f32) {
        match name {
            "room_size" => {
                self.room_size = value.clamp(0.0, 1.0);
                self.rebuild_delays();
            }
            "damping" => self.damping = value.clamp(0.0, 1.0),
            "mix" => self.mix = value.clamp(0.0, 1.0),
            _ => {}
        }
    }

    fn get_params(&self) -> Vec<EffectParam> {
        vec![
            EffectParam::new("room_size", self.room_size, 0.0, 1.0, ""),
            EffectParam::new("damping", self.damping, 0.0, 1.0, ""),
            EffectParam::new("mix", self.mix, 0.0, 1.0, ""),
        ]
    }

    fn set_bypass(&mut self, bypass: bool) {
        self.bypassed = bypass;
    }

    fn is_bypassed(&self) -> bool {
        self.bypassed
    }

    fn set_sample_rate(&mut self, sample_rate: f32) {
        let old_rate = self.sample_rate;
        if (sample_rate - old_rate).abs() < 1.0 {
            return;
        }
        self.sample_rate = sample_rate;
        self.rebuild_delays();
    }
}
