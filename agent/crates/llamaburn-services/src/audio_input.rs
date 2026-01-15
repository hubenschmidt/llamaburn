//! Audio input service for microphone capture
//!
//! Provides device enumeration and audio capture using cpal.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, FromSample, SampleFormat, StreamConfig};
// ringbuf removed - cpal::Stream is not Send, using simple Vec buffer in processing thread
use thiserror::Error;
use tracing::{debug, error, info, warn};

const TARGET_SAMPLE_RATE: u32 = 16000;

#[derive(Debug, Error)]
pub enum AudioInputError {
    #[error("No audio input devices found")]
    NoDevices,
    #[error("Device not found: {0}")]
    DeviceNotFound(String),
    #[error("Failed to get default input config: {0}")]
    ConfigError(String),
    #[error("Failed to build input stream: {0}")]
    StreamError(String),
    #[error("Recording interrupted")]
    Interrupted,
    #[error("Resample error: {0}")]
    ResampleError(String),
}

/// Audio input device information
#[derive(Debug, Clone)]
pub struct AudioDevice {
    /// Raw ALSA device name (e.g., "plughw:CARD=R24,DEV=0")
    pub name: String,
    /// Device ID for selection
    pub id: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub is_default: bool,
    /// Card identifier (e.g., "R24", "Generic_1")
    pub card_id: Option<String>,
    /// Friendly card name from ALSA (e.g., "ZOOM R24", "HD-Audio Generic")
    pub card_name: Option<String>,
    /// Device type hint for display
    pub device_type: DeviceType,
}

/// Device type for grouping and display
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceType {
    /// Direct hardware (hw:)
    Hardware,
    /// Plugin hardware with conversion (plughw:) - recommended
    PluginHardware,
    /// PulseAudio/PipeWire
    PulseAudio,
    /// System default
    Default,
    /// Other (surround, dsnoop, etc.)
    Other,
}

/// Handle to stop a running audio stream
/// Note: Not Send because cpal::Stream is not Send
pub struct StreamHandle {
    stop_flag: Arc<AtomicBool>,
    thread_handle: Option<thread::JoinHandle<()>>,
    #[allow(dead_code)]
    stream: cpal::Stream, // Keep stream alive until handle is dropped
}

impl StreamHandle {
    /// Signal the stream to stop
    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    /// Wait for stream to finish
    pub fn join(mut self) {
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for StreamHandle {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }
}

pub struct AudioInputService;

impl AudioInputService {
    /// Parse /proc/asound/cards for friendly card names
    /// Returns a map of card_id -> friendly_name
    fn parse_alsa_cards() -> std::collections::HashMap<String, String> {
        use std::collections::HashMap;
        use std::fs;

        let mut cards = HashMap::new();

        let Ok(content) = fs::read_to_string("/proc/asound/cards") else {
            return cards;
        };

        // Format:
        //  0 [Generic_1      ]: HDA-Intel - HD-Audio Generic
        //                       HD-Audio Generic at 0xfc600000 irq 72
        //  2 [R24            ]: USB-Audio - ZOOM R24
        for line in content.lines() {
            let line = line.trim();

            // Look for lines with [card_id]: ... - friendly_name
            let Some(bracket_start) = line.find('[') else { continue };
            let Some(bracket_end) = line.find(']') else { continue };
            let Some(dash_pos) = line.find(" - ") else { continue };

            let card_id = line[bracket_start + 1..bracket_end].trim().to_string();
            let friendly_name = line[dash_pos + 3..].trim().to_string();

            cards.insert(card_id, friendly_name);
        }

        cards
    }

    /// Extract card ID from ALSA device name
    /// e.g., "plughw:CARD=R24,DEV=0" -> Some("R24")
    fn extract_card_id(name: &str) -> Option<String> {
        // Look for CARD=xxx pattern
        let card_start = name.find("CARD=")?;
        let after_card = &name[card_start + 5..];

        // Find end (comma or end of string)
        let end = after_card.find(',').unwrap_or(after_card.len());
        Some(after_card[..end].to_string())
    }

    /// Determine device type from name prefix
    fn device_type_from_name(name: &str) -> DeviceType {
        if name.starts_with("plughw:") {
            return DeviceType::PluginHardware;
        }
        if name.starts_with("hw:") {
            return DeviceType::Hardware;
        }
        if name == "pulse" || name == "pipewire" {
            return DeviceType::PulseAudio;
        }
        if name == "default" || name.starts_with("sysdefault:") {
            return DeviceType::Default;
        }
        DeviceType::Other
    }

    /// List all available audio input devices
    pub fn list_devices() -> Result<Vec<AudioDevice>, AudioInputError> {
        let host = cpal::default_host();
        let default_device = host.default_input_device();
        let default_name = default_device.as_ref().and_then(|d| d.name().ok());

        // Get friendly card names from ALSA
        let card_names = Self::parse_alsa_cards();

        let devices: Vec<_> = host
            .input_devices()
            .map_err(|e| AudioInputError::ConfigError(e.to_string()))?
            .filter_map(|device| {
                let name = device.name().ok()?;
                let config = device.default_input_config().ok()?;

                let card_id = Self::extract_card_id(&name);
                let card_name = card_id.as_ref().and_then(|id| card_names.get(id).cloned());
                let device_type = Self::device_type_from_name(&name);

                Some(AudioDevice {
                    id: name.clone(),
                    is_default: default_name.as_ref() == Some(&name),
                    name,
                    sample_rate: config.sample_rate().0,
                    channels: config.channels(),
                    card_id,
                    card_name,
                    device_type,
                })
            })
            .collect();

        if devices.is_empty() {
            return Err(AudioInputError::NoDevices);
        }

        info!(count = devices.len(), "Found audio input devices");
        for device in &devices {
            debug!(
                name = %device.name,
                sample_rate = device.sample_rate,
                channels = device.channels,
                default = device.is_default,
                card_name = ?device.card_name,
                "Audio device"
            );
        }

        Ok(devices)
    }

    /// Get device by ID
    fn get_device(device_id: &str) -> Result<Device, AudioInputError> {
        let host = cpal::default_host();

        // Try to find by name
        for device in host
            .input_devices()
            .map_err(|e| AudioInputError::ConfigError(e.to_string()))?
        {
            if let Ok(name) = device.name() {
                if name == device_id {
                    return Ok(device);
                }
            }
        }

        // Fallback to default
        if device_id == "default" {
            return host
                .default_input_device()
                .ok_or(AudioInputError::NoDevices);
        }

        Err(AudioInputError::DeviceNotFound(device_id.to_string()))
    }

    /// Capture audio for a fixed duration, return 16kHz mono f32 samples
    pub fn capture(
        device_id: &str,
        duration_secs: u32,
    ) -> Result<Vec<f32>, AudioInputError> {
        let device = Self::get_device(device_id)?;
        let config = device
            .default_input_config()
            .map_err(|e| AudioInputError::ConfigError(e.to_string()))?;

        info!(
            device = %device.name().unwrap_or_default(),
            duration = duration_secs,
            sample_rate = config.sample_rate().0,
            channels = config.channels(),
            "Starting audio capture"
        );

        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;
        let expected_samples = (sample_rate * duration_secs) as usize * channels;

        let (tx, rx) = channel::<Vec<f32>>();
        let samples_collected = Arc::new(std::sync::Mutex::new(Vec::with_capacity(expected_samples)));
        let samples_for_callback = samples_collected.clone();

        let stream = match config.sample_format() {
            SampleFormat::F32 => Self::build_stream::<f32>(&device, &config.into(), tx),
            SampleFormat::I16 => Self::build_stream::<i16>(&device, &config.into(), tx),
            SampleFormat::I32 => Self::build_stream::<i32>(&device, &config.into(), tx),
            format => return Err(AudioInputError::ConfigError(format!("Unsupported format: {:?}", format))),
        }?;

        stream.play().map_err(|e| AudioInputError::StreamError(e.to_string()))?;

        // Collect samples for duration
        let deadline = std::time::Instant::now() + Duration::from_secs(duration_secs as u64);
        while std::time::Instant::now() < deadline {
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(100)) {
                let mut samples = samples_for_callback.lock().unwrap();
                samples.extend(chunk);
            }
        }

        drop(stream);

        let raw_samples = samples_collected.lock().unwrap().clone();
        info!(samples = raw_samples.len(), "Capture complete");

        // Convert to mono and resample to 16kHz
        let mono = Self::to_mono(&raw_samples, channels);
        let resampled = Self::resample(&mono, sample_rate, TARGET_SAMPLE_RATE)?;

        Ok(resampled)
    }

    /// Start streaming audio to a channel, returns handle to stop
    pub fn start_stream(
        device_id: &str,
        chunk_tx: Sender<Vec<f32>>,
    ) -> Result<StreamHandle, AudioInputError> {
        let device = Self::get_device(device_id)?;
        let config = device
            .default_input_config()
            .map_err(|e| AudioInputError::ConfigError(e.to_string()))?;

        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        info!(
            device = %device.name().unwrap_or_default(),
            sample_rate = sample_rate,
            channels = channels,
            "Starting audio stream"
        );

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Chunk size for ~500ms of audio
        let chunk_duration_ms = 500;
        let chunk_samples = (sample_rate * chunk_duration_ms / 1000) as usize * channels;

        let (internal_tx, internal_rx) = channel::<Vec<f32>>();

        let stream = match config.sample_format() {
            SampleFormat::F32 => Self::build_stream::<f32>(&device, &config.into(), internal_tx),
            SampleFormat::I16 => Self::build_stream::<i16>(&device, &config.into(), internal_tx),
            SampleFormat::I32 => Self::build_stream::<i32>(&device, &config.into(), internal_tx),
            format => return Err(AudioInputError::ConfigError(format!("Unsupported format: {:?}", format))),
        }?;

        stream.play().map_err(|e| AudioInputError::StreamError(e.to_string()))?;

        // Spawn thread to process and forward audio chunks
        // Stream stays in StreamHandle - its callback runs on cpal's audio thread
        let thread_handle = thread::spawn(move || {
            let mut buffer: Vec<f32> = Vec::with_capacity(chunk_samples * 2);

            while !stop_flag_clone.load(Ordering::SeqCst) {
                // Collect incoming samples
                let Ok(samples) = internal_rx.recv_timeout(Duration::from_millis(50)) else {
                    continue;
                };
                buffer.extend(samples);

                // Send chunks when we have enough samples
                while buffer.len() >= chunk_samples {
                    let chunk: Vec<f32> = buffer.drain(..chunk_samples).collect();

                    // Convert to mono and resample
                    let mono = Self::to_mono(&chunk, channels);
                    let Ok(resampled) = Self::resample(&mono, sample_rate, TARGET_SAMPLE_RATE) else {
                        warn!("Resample error in stream processing");
                        continue;
                    };

                    if chunk_tx.send(resampled).is_err() {
                        return;
                    }
                }
            }

            info!("Audio stream stopped");
        });

        Ok(StreamHandle {
            stop_flag,
            thread_handle: Some(thread_handle),
            stream,
        })
    }

    fn build_stream<T>(
        device: &Device,
        config: &StreamConfig,
        tx: Sender<Vec<f32>>,
    ) -> Result<cpal::Stream, AudioInputError>
    where
        T: cpal::Sample + cpal::SizedSample + Send + 'static,
        f32: FromSample<T>,
    {
        let err_fn = |err| error!("Audio stream error: {}", err);

        device
            .build_input_stream(
                config,
                move |data: &[T], _: &cpal::InputCallbackInfo| {
                    let samples: Vec<f32> = data.iter().map(|s| f32::from_sample_(*s)).collect();
                    let _ = tx.send(samples);
                },
                err_fn,
                None,
            )
            .map_err(|e| AudioInputError::StreamError(e.to_string()))
    }

    /// Convert multi-channel audio to mono
    fn to_mono(samples: &[f32], channels: usize) -> Vec<f32> {
        if channels == 1 {
            return samples.to_vec();
        }

        samples
            .chunks(channels)
            .map(|frame| frame.iter().sum::<f32>() / channels as f32)
            .collect()
    }

    /// Resample audio using rubato
    fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>, AudioInputError> {
        if from_rate == to_rate {
            return Ok(samples.to_vec());
        }

        use rubato::{SincFixedIn, SincInterpolationParameters, SincInterpolationType, Resampler, WindowFunction};

        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };

        let mut resampler = SincFixedIn::<f32>::new(
            to_rate as f64 / from_rate as f64,
            2.0,
            params,
            samples.len(),
            1, // mono
        ).map_err(|e| AudioInputError::ResampleError(e.to_string()))?;

        let input = vec![samples.to_vec()];
        let output = resampler
            .process(&input, None)
            .map_err(|e| AudioInputError::ResampleError(e.to_string()))?;

        Ok(output.into_iter().flatten().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_devices() {
        // This test will fail in CI but works locally
        let result = AudioInputService::list_devices();
        println!("Devices: {:?}", result);
    }
}
