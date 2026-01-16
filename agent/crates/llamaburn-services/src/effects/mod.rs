mod native;

pub use native::{
    CompressorEffect, DelayEffect, GainEffect, HighPassEffect, LowPassEffect, ReverbEffect,
};

use std::fmt::Debug;

/// Audio effect that can process samples in-place
pub trait AudioEffect: Send + Debug {
    fn name(&self) -> &str;
    fn process(&mut self, samples: &mut [f32]);
    fn set_param(&mut self, name: &str, value: f32);
    fn get_params(&self) -> Vec<EffectParam>;
    fn set_bypass(&mut self, bypass: bool);
    fn is_bypassed(&self) -> bool;
}

#[derive(Debug, Clone)]
pub struct EffectParam {
    pub name: String,
    pub value: f32,
    pub min: f32,
    pub max: f32,
    pub unit: String,
}

impl EffectParam {
    pub fn new(name: &str, value: f32, min: f32, max: f32, unit: &str) -> Self {
        Self {
            name: name.to_string(),
            value,
            min,
            max,
            unit: unit.to_string(),
        }
    }
}

/// Chain of audio effects processed in order
#[derive(Debug, Default)]
pub struct EffectChain {
    effects: Vec<Box<dyn AudioEffect>>,
    bypass_all: bool,
}

impl EffectChain {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, effect: Box<dyn AudioEffect>) {
        self.effects.push(effect);
    }

    pub fn remove(&mut self, index: usize) -> Option<Box<dyn AudioEffect>> {
        if index >= self.effects.len() {
            return None;
        }
        Some(self.effects.remove(index))
    }

    pub fn move_up(&mut self, index: usize) {
        if index == 0 || index >= self.effects.len() {
            return;
        }
        self.effects.swap(index, index - 1);
    }

    pub fn move_down(&mut self, index: usize) {
        if index >= self.effects.len() - 1 {
            return;
        }
        self.effects.swap(index, index + 1);
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        if self.bypass_all {
            return;
        }
        for effect in &mut self.effects {
            if !effect.is_bypassed() {
                effect.process(samples);
            }
        }
    }

    pub fn set_bypass_all(&mut self, bypass: bool) {
        self.bypass_all = bypass;
    }

    pub fn is_bypass_all(&self) -> bool {
        self.bypass_all
    }

    pub fn effects(&self) -> &[Box<dyn AudioEffect>] {
        &self.effects
    }

    pub fn effects_mut(&mut self) -> &mut [Box<dyn AudioEffect>] {
        &mut self.effects
    }

    pub fn len(&self) -> usize {
        self.effects.len()
    }

    pub fn is_empty(&self) -> bool {
        self.effects.is_empty()
    }

    pub fn clear(&mut self) {
        self.effects.clear();
    }
}
