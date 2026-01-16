# Audio Effects Specification

## Overview

Add audio effects processing to llamaburn with:
1. **Native Effects** - Built-in DSP effects (EQ, compression, reverb, etc.)
2. **VST Plugin Support** - Load Linux-native VST2 plugins (.so files)

---

## Architecture

### Signal Chain

```
Mic Input → [Effects Chain] → Live Monitor / Recording
                  ↓
         ┌───────────────┐
         │  Effect Slot 1│ → Native or VST
         │  Effect Slot 2│ → Native or VST
         │  Effect Slot N│ → Native or VST
         └───────────────┘
```

### Effect Types

| Category | Native Effects | VST Alternative |
|----------|---------------|-----------------|
| Volume | Gain, Limiter | Any VST |
| EQ | Parametric EQ, High/Low Pass | FabFilter, ReaEQ |
| Dynamics | Compressor, Gate, De-esser | Waves, TDR |
| Time | Reverb, Delay | Valhalla, TAL |
| Pitch | Pitch shift | Antares, Soundtoys |
| Utility | Mono, Phase Invert | - |

---

## Library Recommendations

### Native Effects: fundsp

**Why fundsp:**
- Pure Rust, no FFI complexity
- Real-time safe, zero allocation in audio path
- Composable DSP graphs with operator overloading
- ~50 built-in audio operators

```rust
// Example: Simple channel strip
use fundsp::prelude::*;

let strip = pass()              // Input
    >> highpass_hz(80.0, 1.0)   // HPF at 80Hz
    >> bell_hz(3000.0, 1.0, db_amp(3.0))  // Boost 3kHz
    >> limiter_stereo(0.1, 0.5); // Limiter
```

**Cargo.toml:**
```toml
[dependencies]
fundsp = "0.18"
```

### VST Hosting: vst-rs

**Why vst-rs:**
- Mature VST2 hosting support
- Rust-native API
- Active maintenance

```rust
use vst::plugin::Plugin;
use vst::host::{Host, PluginLoader};

let path = Path::new("/path/to/plugin.vst");
let mut loader = PluginLoader::load(path, host)?;
let mut instance = loader.instance()?;
instance.init();
```

**Cargo.toml:**
```toml
[dependencies]
vst = "0.3"
```

### Windows VST Bridge (Optional): yabridge

If you have Windows VSTs, yabridge can bridge them to Linux:
- Uses Wine under the hood
- Supports VST2 and VST3

```bash
yay -S yabridge  # Arch
yabridgectl add /path/to/windows/plugins && yabridgectl sync
```

---

## Implementation Plan

### Phase 1: Native Effects Infrastructure

**New Files:**
- `llamaburn-services/src/effects/mod.rs` - Effect trait, chain management
- `llamaburn-services/src/effects/native.rs` - Built-in effects using fundsp
- `llamaburn-services/src/effects/chain.rs` - Effect chain processor

**Effect Trait:**
```rust
pub trait AudioEffect: Send {
    fn name(&self) -> &str;
    fn process(&mut self, samples: &mut [f32]);
    fn set_param(&mut self, name: &str, value: f32);
    fn get_params(&self) -> Vec<(String, f32, f32, f32)>; // name, value, min, max
    fn bypass(&mut self, bypass: bool);
}
```

**Effect Chain:**
```rust
pub struct EffectChain {
    effects: Vec<Box<dyn AudioEffect>>,
    bypass_all: bool,
}

impl EffectChain {
    pub fn process(&mut self, samples: &mut [f32]) {
        if self.bypass_all { return; }
        for effect in &mut self.effects {
            effect.process(samples);
        }
    }
}
```

### Phase 2: VST Plugin Hosting

**New Files:**
- `llamaburn-services/src/effects/vst_host.rs` - VST2 plugin loader
- `llamaburn-services/src/effects/vst_effect.rs` - VST wrapper implementing AudioEffect

**VST Effect Wrapper:**
```rust
pub struct VstEffect {
    instance: PluginInstance,
    name: String,
    bypassed: bool,
}

impl AudioEffect for VstEffect {
    fn process(&mut self, samples: &mut [f32]) {
        if self.bypassed { return; }
        // Convert to VST buffer format and process
        self.instance.process(&mut audio_buffer);
    }
}
```

### Phase 3: GUI Integration

**Modify:** `llamaburn-gui/src/panels/benchmark.rs`

**Add to Audio Setup menu:**
```
🔊 Audio Setup ▼
├─ Recording Device ►
├─ ─────────────────
├─ 🎛️ Effects Chain ►
│   ├─ [+] Add Effect ►
│   │   ├─ Native Effects ►
│   │   │   ├─ Gain
│   │   │   ├─ Compressor
│   │   │   ├─ EQ
│   │   │   └─ ...
│   │   └─ Load VST... (file picker)
│   ├─ ─────────────
│   ├─ 1. Compressor [Edit] [X]
│   ├─ 2. EQ [Edit] [X]
│   └─ ─────────────
│       [Bypass All]
├─ ─────────────────
├─ 🎙️ Test Mic
└─ 🎧 Live Monitor
```

---

## Files to Modify/Create

| File | Action |
|------|--------|
| `llamaburn-services/Cargo.toml` | Add fundsp, vst dependencies |
| `llamaburn-services/src/effects/mod.rs` | New: Effect trait, chain |
| `llamaburn-services/src/effects/native.rs` | New: Built-in effects |
| `llamaburn-services/src/effects/vst_host.rs` | New: VST loader |
| `llamaburn-services/src/lib.rs` | Export effects module |
| `llamaburn-gui/src/panels/benchmark.rs` | Effects UI, chain integration |

---

## Supported Plugin Formats

| Format | Support |
|--------|---------|
| Linux VST2 (.so) | Native via vst-rs ✓ |
| Linux VST3 (.vst3) | Via vst3-sys (future) |
| Windows VST | Via yabridge (optional) |

### Recommended Linux VST Sources

- **Free:** Vital, Surge XT, Dexed, OB-Xd, TAL plugins, LSP plugins
- **Commercial:** u-he, FabFilter, Bitwig (all have Linux builds)

---

## Verification

1. Add a native Gain effect → hear volume change in live monitor
2. Load a Linux VST2 plugin (.so) → effect applies to audio
3. Reorder effects in chain → hear difference in sound
4. Bypass all → clean passthrough

---

## Dependencies

```toml
[dependencies]
fundsp = "0.18"       # Native DSP
vst = "0.3"           # VST2 hosting
```

---

## Open Questions

1. **VST3 Support?** - vst-rs is VST2 only. VST3 would need vst3-sys or nih-plug
2. **Preset Management?** - Save/load effect chain configurations?
3. **Per-effect GUI?** - Open VST plugin editor windows?
