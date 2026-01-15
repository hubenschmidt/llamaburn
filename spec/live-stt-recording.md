# Live STT Recording Specification

**Parent:** [audio-benchmarking.md](./audio-benchmarking.md)
**Status:** Draft
**Priority:** Phase 1 Enhancement

---

## Overview

Add live microphone recording to STT benchmarking with real-time GPU monitoring. Support any audio input device (USB interfaces, built-in mics, Zoom R24, etc.).

---

## User Stories

1. **As a user**, I want to select my Zoom R24 audio interface from a dropdown so I can record high-quality audio for benchmarking.

2. **As a user**, I want to "Record Live" and see transcription appear in real-time as I speak, with GPU utilization displayed alongside.

3. **As a user**, I want to "Capture Recording" for a fixed duration, then benchmark the captured audio with multiple iterations.

---

## Two Recording Modes

### Mode 1: Record Live (Streaming)

```
┌──────────────────────────────────────────────────────┐
│  🎤 LIVE RECORDING                                   │
│  ────────────────────────────────────────────────── │
│  Device: [Zoom R24: Line In ▼]                       │
│  Model:  [Medium ▼]                                  │
│                                                      │
│  [🔴 Start Recording]  [⬛ Stop]                     │
│  ────────────────────────────────────────────────── │
│  Live Transcription:                                 │
│  ┌────────────────────────────────────────────────┐ │
│  │ "Hello, this is a test of the live            │ │
│  │  transcription system running on whisper..."  │ │
│  └────────────────────────────────────────────────┘ │
│                                                      │
│  RTF: 0.72x | VRAM: 4.2GB | GPU: 78%                │
│  Duration: 00:15.3                                   │
└──────────────────────────────────────────────────────┘
```

**Behavior:**
- Audio streams to Whisper in ~5-second chunks
- Partial transcription displayed as chunks complete
- GPU metrics polled every 100ms and displayed live
- No warmup/iteration - single continuous benchmark
- Results saved to history when stopped

### Mode 2: Capture Recording (Buffered)

```
┌──────────────────────────────────────────────────────┐
│  🎙️ CAPTURE RECORDING                               │
│  ────────────────────────────────────────────────── │
│  Device:     [Zoom R24: Line In ▼]                   │
│  Model:      [Medium ▼]                              │
│  Duration:   [30] seconds                            │
│  Iterations: [5]                                     │
│  Warmup:     [1]                                     │
│                                                      │
│  [🔴 Start Recording]                                │
│  ────────────────────────────────────────────────── │
│  Status: Recording... 00:12.5 / 00:30.0              │
│  ▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░░ 42%               │
└──────────────────────────────────────────────────────┘
```

**Behavior:**
- Records for specified duration to memory buffer
- After recording, runs standard benchmark (warmup + iterations)
- GPU metrics captured during each iteration
- Full metrics summary displayed (avg/min/max RTF)
- Results saved to history

---

## Audio Device Detection

### Requirements

- Enumerate all ALSA/PulseAudio input devices
- Display human-readable names (not just "hw:2,0")
- Show sample rate and channel count
- Support hot-plugging (refresh button)

### Example Device List

```
┌────────────────────────────────────────────────────┐
│ Audio Input Device                                  │
├────────────────────────────────────────────────────┤
│ ▶ Zoom R24: Line In (hw:2,0)      48kHz stereo     │
│   Built-in Audio: Mic (hw:0,0)    44.1kHz mono    │
│   USB Microphone (hw:3,0)         48kHz mono      │
│   PulseAudio Default              44.1kHz stereo  │
└────────────────────────────────────────────────────┘
```

---

## GPU Monitoring During Recording

### Metrics to Display

| Metric | Source | Update Rate |
|--------|--------|-------------|
| VRAM Used | `rocm-smi --showmeminfo vram` | 100ms |
| GPU Utilization | `rocm-smi --showuse` | 100ms |
| Temperature | `rocm-smi --showtemp` | 1s |
| Power Draw | `rocm-smi --showpower` | 1s |

### Display Format

```
GPU: ████████░░ 78% | VRAM: 4.2/16GB | Temp: 65°C | Power: 180W
```

### Implementation

- Spawn dedicated GPU monitor thread during recording
- Higher polling rate (100ms) vs normal (1s)
- Interleave GPU events with transcription events
- Calculate average GPU usage over recording duration

---

## Data Structures

### AudioSource Enum

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioSource {
    /// Load from file (existing behavior)
    File(PathBuf),

    /// Record for fixed duration, then benchmark
    Capture {
        device_id: String,
        duration_secs: u32,
    },

    /// Stream live to Whisper in real-time
    LiveStream {
        device_id: String,
    },
}
```

### AudioDevice

```rust
#[derive(Debug, Clone)]
pub struct AudioDevice {
    pub name: String,           // "Zoom R24: Line In"
    pub id: String,             // "hw:2,0" or cpal device name
    pub sample_rate: u32,       // Native rate (will resample to 16kHz)
    pub channels: u16,          // 1 or 2
    pub is_default: bool,
}
```

### LiveBenchmarkMetrics

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveBenchmarkMetrics {
    pub rtf: f64,
    pub processing_time_ms: f64,
    pub audio_duration_ms: f64,
    pub word_count: u32,

    // GPU metrics (averaged over recording)
    pub avg_gpu_utilization: f32,
    pub peak_vram_mb: u64,
    pub avg_power_watts: f32,
}
```

---

## Technical Architecture

### Audio Pipeline

```
┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│  cpal       │────▶│  Resampler   │────▶│  Ring       │
│  Callback   │     │  (rubato)    │     │  Buffer     │
│  48kHz      │     │  →16kHz mono │     │  ~5s chunks │
└─────────────┘     └──────────────┘     └──────┬──────┘
                                                │
                    ┌──────────────┐            │
                    │  Whisper     │◀───────────┘
                    │  Context     │
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │  mpsc        │
                    │  Channel     │
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │  UI Update   │
                    └──────────────┘
```

### Thread Model

```
Main Thread (UI)
├── Audio Capture Thread (cpal callback → ring buffer)
├── Whisper Processing Thread (consume buffer → transcribe)
└── GPU Monitor Thread (poll rocm-smi → metrics)
```

### Event Channel

```rust
pub enum LiveRecordingEvent {
    // Audio capture events
    RecordingStarted,
    AudioChunkCaptured { duration_ms: u64 },
    RecordingStopped { total_duration_ms: u64 },

    // Transcription events
    PartialTranscription { text: String, rtf: f64 },
    FinalTranscription { text: String, rtf: f64 },

    // GPU events
    GpuMetrics { vram_mb: u64, utilization: f32, temp_c: u32 },

    // Error events
    Error(String),
}
```

---

## Dependencies

### New Crates

```toml
# Audio input (cross-platform)
cpal = "0.15"

# Ring buffer for audio chunks
ringbuf = "0.4"
```

### Existing (Already in Use)

- `rubato` - Resampling to 16kHz
- `whisper-rs` - Transcription
- `hound` - WAV export (for captured audio)

---

## UI Changes

### Audio Setup Button (Audacity-style)

Add a toolbar-style "Audio Setup" button with dropdown menu for quick device configuration.

```
┌────────────────────────────────────────────────────────────────┐
│  Audio Benchmark                                               │
│  ──────────────────────────────────────────────────────────── │
│  [🔊 Audio Setup ▼]                                            │
│  ┌─────────────────────────────────────┐                       │
│  │ Recording Device          ▶         │ ┌─────────────────────────────────┐
│  │ Recording Channels        ▶         │ │ • Zoom R24: Line In (hw:2,0)    │
│  │ ─────────────────────────────────── │ │   HD-Audio Generic (hw:0,0)     │
│  │ Rescan Audio Devices                │ │   pipewire                       │
│  │ Audio Settings...                   │ │   pulse                          │
│  └─────────────────────────────────────┘ │   default                        │
│                                          └─────────────────────────────────┘
│  Model: [Medium ▼]                                             │
│  Source: [File] [Capture] [Live]                               │
└────────────────────────────────────────────────────────────────┘
```

**Menu Items:**

| Item | Action |
|------|--------|
| Recording Device | Submenu listing all input devices |
| Recording Channels | Submenu: Mono / Stereo |
| Rescan Audio Devices | Refresh device list (hot-plug support) |
| Audio Settings... | Open settings dialog (optional, future) |

**Behavior:**
- Button shows current device name when selected
- Checkmark (•) indicates currently selected device
- Greyed out items when recording is active
- Menu closes after selection

### Audio Settings Dialog (Future)

```
┌─────────────────────── Audio Settings ───────────────────────┐
│                                                               │
│  Recording                                                    │
│  ─────────────────────────────────────────────────────────── │
│  Device:    [Zoom R24: Line In (hw:2,0)           ▼]         │
│  Channels:  [2 (Stereo)                           ▼]         │
│                                                               │
│  Quality                                                      │
│  ─────────────────────────────────────────────────────────── │
│  Sample Rate: 16000 Hz (Whisper native)                       │
│  Format:      32-bit float                                    │
│                                                               │
│                                    [Cancel]  [OK]             │
└───────────────────────────────────────────────────────────────┘
```

### BenchmarkPanel State Additions

```rust
// Audio device selection
audio_devices: Vec<AudioDevice>,
selected_device: Option<String>,
loading_devices: bool,

// Recording state
recording_mode: RecordingMode,
is_recording: bool,
recording_duration_secs: u32,
live_recording_rx: Option<Receiver<LiveRecordingEvent>>,

// Live output
live_transcription: String,
live_rtf: f64,
live_gpu_metrics: Option<GpuMetrics>,
```

### RecordingMode Enum

```rust
#[derive(Default, PartialEq)]
enum RecordingMode {
    #[default]
    File,       // Existing file picker
    LiveStream, // Real-time streaming
    Capture,    // Record then benchmark
}
```

---

## Verification Checklist

- [ ] `arecord -l` lists Zoom R24 as capture device
- [ ] cpal enumerates the device correctly
- [ ] Device dropdown shows human-readable names
- [ ] Capture mode: Record 10s → benchmark runs → RTF displayed
- [ ] Live mode: Speak → text appears in <2s latency
- [ ] GPU metrics update during recording
- [ ] Results saved to history with device info
- [ ] Hot-plug: Plug in USB mic → refresh → device appears

---

## Future Enhancements

- Voice activity detection (VAD) - auto-stop on silence
- Audio level meter visualization
- Save captured audio as WAV file
- WER calculation against expected text
- Multi-device recording (A/B comparison)
