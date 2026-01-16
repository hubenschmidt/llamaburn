//! Whisper Speech-to-Text service
//!
//! Requires the `whisper` feature to be enabled.
//! For ROCm GPU support, use the `whisper-gpu` feature.
//!
//! Install system dependencies: `sudo apt install cmake clang`

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::time::Duration;
#[cfg(feature = "whisper")]
use std::time::Instant;

use thiserror::Error;
#[cfg(feature = "whisper")]
use tracing::{debug, info, warn};

use llamaburn_core::{AudioBenchmarkMetrics, WhisperModel};

#[derive(Error, Debug)]
pub enum WhisperError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Failed to load model: {0}")]
    ModelLoadError(String),
    #[error("Failed to load audio: {0}")]
    AudioLoadError(String),
    #[error("Transcription failed: {0}")]
    TranscriptionError(String),
    #[error("Audio format not supported: {0}")]
    UnsupportedFormat(String),
    #[error("Whisper feature not enabled")]
    FeatureNotEnabled,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone)]
pub struct TranscriptionResult {
    pub text: String,
    pub segments: Vec<Segment>,
    pub language: String,
}

#[derive(Debug, Clone)]
pub struct Segment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub enum WhisperEvent {
    LoadingModel { model: WhisperModel },
    ModelLoaded { load_time_ms: u64 },
    LoadingAudio { path: PathBuf },
    AudioLoaded { duration_ms: u64 },
    Transcribing,
    TranscriptionComplete { result: TranscriptionResult, processing_ms: u64 },
    Error { message: String },
}

pub struct WhisperService {
    model_dir: PathBuf,
    current_model: Option<WhisperModel>,
    #[cfg(feature = "whisper")]
    context: Option<whisper_rs::WhisperContext>,
}

impl WhisperService {
    pub fn new(model_dir: &Path) -> Self {
        std::fs::create_dir_all(model_dir).ok();
        Self {
            model_dir: model_dir.to_path_buf(),
            current_model: None,
            #[cfg(feature = "whisper")]
            context: None,
        }
    }

    pub fn default_model_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("llamaburn")
            .join("whisper")
    }

    pub fn model_path(&self, model: WhisperModel) -> PathBuf {
        self.model_dir.join(model.filename())
    }

    pub fn is_model_downloaded(&self, model: WhisperModel) -> bool {
        self.model_path(model).exists()
    }

    pub fn is_whisper_enabled() -> bool {
        cfg!(feature = "whisper")
    }

    #[cfg(feature = "whisper")]
    pub fn load_model(&mut self, model: WhisperModel) -> Result<Duration, WhisperError> {
        let path = self.model_path(model);

        if !path.exists() {
            return Err(WhisperError::ModelNotFound(format!(
                "Model {} not found at {}. Download from {}",
                model.label(),
                path.display(),
                model.download_url()
            )));
        }

        info!("Loading Whisper model: {} from {}", model.label(), path.display());
        let start = Instant::now();

        let ctx = whisper_rs::WhisperContext::new_with_params(
            path.to_str().ok_or_else(|| WhisperError::ModelLoadError("Invalid path".into()))?,
            whisper_rs::WhisperContextParameters::default(),
        )
        .map_err(|e| WhisperError::ModelLoadError(e.to_string()))?;

        let elapsed = start.elapsed();
        info!("Model loaded in {:?}", elapsed);

        self.context = Some(ctx);
        self.current_model = Some(model);

        Ok(elapsed)
    }

    #[cfg(not(feature = "whisper"))]
    pub fn load_model(&mut self, _model: WhisperModel) -> Result<Duration, WhisperError> {
        Err(WhisperError::FeatureNotEnabled)
    }

    pub fn current_model(&self) -> Option<WhisperModel> {
        self.current_model
    }

    /// Unload the currently loaded model to free memory
    pub fn unload_model(&mut self) {
        self.current_model = None;
        #[cfg(feature = "whisper")]
        {
            self.context = None;
        }
    }

    #[cfg(feature = "whisper")]
    pub fn transcribe(&self, audio_path: &Path) -> Result<TranscriptionResult, WhisperError> {
        let (result, _) = self.transcribe_with_timing(audio_path)?;
        Ok(result)
    }

    #[cfg(not(feature = "whisper"))]
    pub fn transcribe(&self, _audio_path: &Path) -> Result<TranscriptionResult, WhisperError> {
        Err(WhisperError::FeatureNotEnabled)
    }

    #[cfg(feature = "whisper")]
    pub fn transcribe_with_timing(
        &self,
        audio_path: &Path,
    ) -> Result<(TranscriptionResult, Duration), WhisperError> {
        let ctx = self
            .context
            .as_ref()
            .ok_or_else(|| WhisperError::ModelLoadError("No model loaded".into()))?;

        // Load and convert audio to 16kHz mono
        let audio_data = self.load_audio(audio_path)?;
        debug!("Audio loaded: {} samples", audio_data.len());

        // Configure whisper
        let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Run transcription
        let start = Instant::now();
        let mut state = ctx
            .create_state()
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        state
            .full(params, &audio_data)
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        let elapsed = start.elapsed();

        // Extract segments
        let num_segments = state.full_n_segments();

        let mut segments = Vec::new();
        let mut full_text = String::new();

        for i in 0..num_segments {
            let Some(seg) = state.get_segment(i) else {
                continue;
            };

            let text = seg.to_str()
                .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?
                .to_string();

            full_text.push_str(&text);
            segments.push(Segment {
                start_ms: seg.start_timestamp() * 10, // centiseconds to ms
                end_ms: seg.end_timestamp() * 10,
                text,
            });
        }

        let result = TranscriptionResult {
            text: full_text.trim().to_string(),
            segments,
            language: "en".to_string(),
        };

        Ok((result, elapsed))
    }

    #[cfg(not(feature = "whisper"))]
    pub fn transcribe_with_timing(
        &self,
        _audio_path: &Path,
    ) -> Result<(TranscriptionResult, Duration), WhisperError> {
        Err(WhisperError::FeatureNotEnabled)
    }

    /// Transcribe from raw 16kHz mono f32 samples (for captured microphone audio)
    #[cfg(feature = "whisper")]
    pub fn transcribe_samples(&self, samples: &[f32]) -> Result<(TranscriptionResult, Duration), WhisperError> {
        use std::time::Instant;

        let ctx = self
            .context
            .as_ref()
            .ok_or_else(|| WhisperError::ModelLoadError("No model loaded".into()))?;

        debug!("Transcribing {} samples", samples.len());

        // Configure whisper
        let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_timestamps(false);

        // Run transcription
        let start = Instant::now();
        let mut state = ctx
            .create_state()
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        state
            .full(params, samples)
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        let elapsed = start.elapsed();

        // Extract segments
        let num_segments = state.full_n_segments();
        let mut segments = Vec::new();
        let mut full_text = String::new();

        for i in 0..num_segments {
            let Some(seg) = state.get_segment(i) else {
                continue;
            };

            let text = seg.to_str()
                .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?
                .to_string();

            full_text.push_str(&text);
            segments.push(Segment {
                start_ms: seg.start_timestamp() * 10,
                end_ms: seg.end_timestamp() * 10,
                text,
            });
        }

        let result = TranscriptionResult {
            text: full_text.trim().to_string(),
            segments,
            language: "en".to_string(),
        };

        Ok((result, elapsed))
    }

    #[cfg(not(feature = "whisper"))]
    pub fn transcribe_samples(&self, _samples: &[f32]) -> Result<(TranscriptionResult, Duration), WhisperError> {
        Err(WhisperError::FeatureNotEnabled)
    }

    /// Transcribe with verbose output and streaming segment callback
    #[cfg(feature = "whisper")]
    pub fn transcribe_samples_streaming(
        &self,
        samples: &[f32],
        segment_tx: std::sync::mpsc::Sender<String>,
    ) -> Result<(TranscriptionResult, Duration), WhisperError> {
        use std::time::Instant;

        let ctx = self
            .context
            .as_ref()
            .ok_or_else(|| WhisperError::ModelLoadError("No model loaded".into()))?;

        debug!("Transcribing {} samples (streaming)", samples.len());

        // Configure whisper with verbose output
        let mut params = whisper_rs::FullParams::new(whisper_rs::SamplingStrategy::Greedy { best_of: 1 });
        params.set_language(Some("en"));
        params.set_print_special(true);
        params.set_print_progress(true);
        params.set_print_realtime(true);
        params.set_print_timestamps(true);

        // Set up segment callback for streaming output
        let tx = segment_tx.clone();
        params.set_segment_callback_safe_lossy(move |segment: whisper_rs::SegmentCallbackData| {
            let text = segment.text.trim();
            if !text.is_empty() {
                let _ = tx.send(format!(
                    "[{:.2}s → {:.2}s] {}",
                    segment.start_timestamp as f64 / 100.0,
                    segment.end_timestamp as f64 / 100.0,
                    text
                ));
            }
        });

        // Run transcription
        let start = Instant::now();
        let mut state = ctx
            .create_state()
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        state
            .full(params, samples)
            .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?;

        let elapsed = start.elapsed();

        // Extract final segments
        let num_segments = state.full_n_segments();
        let mut segments = Vec::new();
        let mut full_text = String::new();

        for i in 0..num_segments {
            let Some(seg) = state.get_segment(i) else {
                continue;
            };

            let text = seg.to_str()
                .map_err(|e| WhisperError::TranscriptionError(e.to_string()))?
                .to_string();

            full_text.push_str(&text);
            segments.push(Segment {
                start_ms: seg.start_timestamp() * 10,
                end_ms: seg.end_timestamp() * 10,
                text,
            });
        }

        let result = TranscriptionResult {
            text: full_text.trim().to_string(),
            segments,
            language: "en".to_string(),
        };

        Ok((result, elapsed))
    }

    #[cfg(not(feature = "whisper"))]
    pub fn transcribe_samples_streaming(
        &self,
        _samples: &[f32],
        _segment_tx: std::sync::mpsc::Sender<String>,
    ) -> Result<(TranscriptionResult, Duration), WhisperError> {
        Err(WhisperError::FeatureNotEnabled)
    }

    #[cfg(feature = "whisper")]
    fn load_audio(&self, path: &Path) -> Result<Vec<f32>, WhisperError> {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();

        match ext.as_str() {
            "wav" => self.load_wav(path),
            "mp3" | "flac" | "m4a" | "aac" | "ogg" => self.load_with_symphonia(path),
            _ => Err(WhisperError::UnsupportedFormat(ext)),
        }
    }

    #[cfg(feature = "whisper")]
    fn load_wav(&self, path: &Path) -> Result<Vec<f32>, WhisperError> {
        let reader = hound::WavReader::open(path)
            .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

        let spec = reader.spec();
        debug!(
            "WAV: {} Hz, {} channels, {:?}",
            spec.sample_rate, spec.channels, spec.sample_format
        );

        let samples = self.read_wav_samples(reader, &spec)?;
        let mono = self.to_mono(&samples, spec.channels);

        if spec.sample_rate == 16000 {
            return Ok(mono);
        }

        self.resample(&mono, spec.sample_rate, 16000)
    }

    #[cfg(feature = "whisper")]
    fn read_wav_samples(
        &self,
        reader: hound::WavReader<std::io::BufReader<std::fs::File>>,
        spec: &hound::WavSpec,
    ) -> Result<Vec<f32>, WhisperError> {
        if spec.sample_format == hound::SampleFormat::Float {
            return Ok(reader.into_samples::<f32>().filter_map(Result::ok).collect());
        }

        let bits = spec.bits_per_sample;
        let max = (1 << (bits - 1)) as f32;
        Ok(reader
            .into_samples::<i32>()
            .filter_map(Result::ok)
            .map(|s| s as f32 / max)
            .collect())
    }

    #[cfg(feature = "whisper")]
    fn to_mono(&self, samples: &[f32], channels: u16) -> Vec<f32> {
        if channels == 1 {
            return samples.to_vec();
        }

        samples
            .chunks(channels as usize)
            .map(|c| c.iter().sum::<f32>() / channels as f32)
            .collect()
    }

    #[cfg(feature = "whisper")]
    fn load_with_symphonia(&self, path: &Path) -> Result<Vec<f32>, WhisperError> {
        use symphonia::core::codecs::DecoderOptions;
        use symphonia::core::formats::FormatOptions;
        use symphonia::core::io::MediaSourceStream;
        use symphonia::core::meta::MetadataOptions;
        use symphonia::core::probe::Hint;

        let file = std::fs::File::open(path)
            .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();

        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

        let mut format = probed.format;
        let track = format
            .default_track()
            .ok_or_else(|| WhisperError::AudioLoadError("No audio track found".into()))?;

        let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
        let channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(2);

        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())
            .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

        let track_id = track.id;
        let samples = self.decode_all_packets(&mut format, &mut decoder, track_id)?;
        let mono = self.to_mono(&samples, channels as u16);

        if sample_rate == 16000 {
            return Ok(mono);
        }

        self.resample(&mono, sample_rate, 16000)
    }

    #[cfg(feature = "whisper")]
    fn decode_all_packets(
        &self,
        format: &mut Box<dyn symphonia::core::formats::FormatReader>,
        decoder: &mut Box<dyn symphonia::core::codecs::Decoder>,
        track_id: u32,
    ) -> Result<Vec<f32>, WhisperError> {
        use symphonia::core::audio::SampleBuffer;

        let mut samples: Vec<f32> = Vec::new();

        loop {
            let packet = match format.next_packet() {
                Ok(p) => p,
                Err(symphonia::core::errors::Error::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(samples),
                Err(e) => {
                    warn!("Error reading packet: {}", e);
                    return Ok(samples);
                }
            };

            if packet.track_id() != track_id {
                continue;
            }

            let decoded = match decoder.decode(&packet) {
                Ok(d) => d,
                Err(e) => {
                    warn!("Error decoding packet: {}", e);
                    continue;
                }
            };

            let spec = *decoded.spec();
            let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
            sample_buf.copy_interleaved_ref(decoded);
            samples.extend(sample_buf.samples());
        }
    }

    #[cfg(feature = "whisper")]
    fn resample(&self, input: &[f32], from_rate: u32, to_rate: u32) -> Result<Vec<f32>, WhisperError> {
        use rubato::{FftFixedIn, Resampler};

        let chunk_size = 1024;

        let mut resampler = FftFixedIn::<f32>::new(from_rate as usize, to_rate as usize, chunk_size, 2, 1)
            .map_err(|e| WhisperError::AudioLoadError(format!("Resampler init failed: {}", e)))?;

        let mut output = Vec::new();
        let mut pos = 0;

        while pos < input.len() {
            let end = (pos + chunk_size).min(input.len());
            let chunk = &input[pos..end];

            // Pad if needed
            let padded: Vec<f32> = if chunk.len() < chunk_size {
                let mut p = chunk.to_vec();
                p.resize(chunk_size, 0.0);
                p
            } else {
                chunk.to_vec()
            };

            let resampled = resampler
                .process(&[padded], None)
                .map_err(|e| WhisperError::AudioLoadError(format!("Resample failed: {}", e)))?;

            output.extend(&resampled[0]);
            pos += chunk_size;
        }

        debug!(
            "Resampled {} samples at {}Hz to {} samples at {}Hz",
            input.len(), from_rate, output.len(), to_rate
        );

        Ok(output)
    }

    #[cfg(feature = "whisper")]
    pub fn run_benchmark(
        &mut self,
        model: WhisperModel,
        audio_path: &Path,
        iterations: u32,
        warmup: u32,
        tx: Option<Sender<WhisperEvent>>,
    ) -> Result<Vec<AudioBenchmarkMetrics>, WhisperError> {
        let send = |event: WhisperEvent| {
            if let Some(ref tx) = tx {
                let _ = tx.send(event);
            }
        };

        // Load model if needed
        if self.current_model != Some(model) {
            send(WhisperEvent::LoadingModel { model });
            let load_time = self.load_model(model)?;
            send(WhisperEvent::ModelLoaded {
                load_time_ms: load_time.as_millis() as u64,
            });
        }

        // Load audio once
        send(WhisperEvent::LoadingAudio {
            path: audio_path.to_path_buf(),
        });
        let audio_data = self.load_audio(audio_path)?;
        let audio_duration_ms = (audio_data.len() as f64 / 16.0) as f64; // 16kHz = 16 samples/ms
        send(WhisperEvent::AudioLoaded {
            duration_ms: audio_duration_ms as u64,
        });

        // Warmup runs
        for i in 0..warmup {
            debug!("Warmup run {}/{}", i + 1, warmup);
            let _ = self.transcribe(audio_path)?;
        }

        // Benchmark runs
        let mut metrics = Vec::with_capacity(iterations as usize);

        for i in 0..iterations {
            send(WhisperEvent::Transcribing);

            let (result, duration) = self.transcribe_with_timing(audio_path)?;
            let processing_ms = duration.as_secs_f64() * 1000.0;
            let rtf = processing_ms / audio_duration_ms;

            send(WhisperEvent::TranscriptionComplete {
                result: result.clone(),
                processing_ms: processing_ms as u64,
            });

            metrics.push(AudioBenchmarkMetrics {
                real_time_factor: rtf,
                processing_time_ms: processing_ms,
                audio_duration_ms,
                transcription: result.text,
                word_count: result.segments.len() as u32,
            });

            debug!(
                "Iteration {}/{}: RTF={:.3}, time={:.1}ms",
                i + 1, iterations, rtf, processing_ms
            );
        }

        Ok(metrics)
    }

    #[cfg(not(feature = "whisper"))]
    pub fn run_benchmark(
        &mut self,
        _model: WhisperModel,
        _audio_path: &Path,
        _iterations: u32,
        _warmup: u32,
        _tx: Option<Sender<WhisperEvent>>,
    ) -> Result<Vec<AudioBenchmarkMetrics>, WhisperError> {
        Err(WhisperError::FeatureNotEnabled)
    }
}

impl Default for WhisperService {
    fn default() -> Self {
        Self::new(&Self::default_model_dir())
    }
}

pub fn get_audio_duration_ms(path: &Path) -> Result<f64, WhisperError> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if ext == "wav" {
        return get_wav_duration_ms(path);
    }

    get_symphonia_duration_ms(path, &ext)
}

fn get_wav_duration_ms(path: &Path) -> Result<f64, WhisperError> {
    let reader = hound::WavReader::open(path)
        .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

    let spec = reader.spec();
    let samples = reader.len() as f64;
    let duration_sec = samples / spec.channels as f64 / spec.sample_rate as f64;

    Ok(duration_sec * 1000.0)
}

fn get_symphonia_duration_ms(path: &Path, ext: &str) -> Result<f64, WhisperError> {
    use symphonia::core::formats::FormatOptions;
    use symphonia::core::io::MediaSourceStream;
    use symphonia::core::meta::MetadataOptions;
    use symphonia::core::probe::Hint;

    let file = std::fs::File::open(path)
        .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    hint.with_extension(ext);

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| WhisperError::AudioLoadError(e.to_string()))?;

    let track = probed
        .format
        .default_track()
        .ok_or_else(|| WhisperError::AudioLoadError("No audio track".into()))?;

    let n_frames = track
        .codec_params
        .n_frames
        .ok_or_else(|| WhisperError::AudioLoadError("Cannot determine duration".into()))?;

    let sample_rate = track.codec_params.sample_rate.unwrap_or(44100);
    let duration_sec = n_frames as f64 / sample_rate as f64;

    Ok(duration_sec * 1000.0)
}
