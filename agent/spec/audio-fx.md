# Audio Effects Specification

## Overview

Add audio effects processing to llamaburn with:
1. **Native Effects** - Built-in DSP effects (EQ, compression, reverb, etc.)
2. **VST Plugin Support** - Load external VST2/VST3 plugins
3. **32-bit Plugin Compatibility** - Support legacy 32-bit plugins via bridge

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

### 32-bit Plugin Bridge: yabridge

**Why yabridge:**
- Runs 32-bit Windows VSTs on 64-bit Linux
- Uses Wine under the hood
- Near-native performance
- Supports VST2 and VST3

**Setup:**
```bash
# Install yabridge
yay -S yabridge  # Arch
# or download from https://github.com/robbert-vdh/yabridge

# Bridge 32-bit plugins
yabridgectl add ~/.wine/drive_c/Program\ Files\ \(x86\)/VstPlugins
yabridgectl sync
```

After bridging, 32-bit plugins appear as Linux-native `.so` files.

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

## 32-bit Plugin Compatibility

### Summary

| Plugin Type | Linux x64 Support |
|-------------|-------------------|
| 64-bit Linux VST | Native ✓ |
| 64-bit Windows VST | Via yabridge + Wine ✓ |
| 32-bit Linux VST | Not supported (rare) |
| 32-bit Windows VST | Via yabridge + Wine ✓ |

**Your 15-year-old 32-bit plugins will work** if you:
1. Install yabridge and Wine
2. Bridge the plugin directory
3. Load the resulting `.so` file in llamaburn

### Alternative: Carla

If vst-rs proves difficult, Carla (KXStudio) is a mature plugin host that:
- Supports VST2, VST3, LV2, LADSPA
- Has 32-bit bridging built-in
- Can be embedded or used as JACK client

---

## Verification

1. Add a native Gain effect → hear volume change in live monitor
2. Load a 64-bit VST → effect applies to audio
3. Bridge a 32-bit Windows VST with yabridge → loads and processes
4. Reorder effects in chain → hear difference in sound
5. Bypass all → clean passthrough

---

## Dependencies

```toml
[dependencies]
fundsp = "0.18"       # Native DSP
vst = "0.3"           # VST2 hosting

[target.'cfg(unix)'.dependencies]
# yabridge is external tool, not a crate
```

---

## Open Questions

1. **VST3 Support?** - vst-rs is VST2 only. VST3 would need vst3-sys or nih-plug
2. **Preset Management?** - Save/load effect chain configurations?
3. **Per-effect GUI?** - Open VST plugin editor windows?
