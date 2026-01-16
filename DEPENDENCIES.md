# LlamaBurn Dependencies

## Build Dependencies

### Rust Toolchain
- Rust 1.92+ (auto-installed via `rust-toolchain.toml`)

### System Packages (Ubuntu/Debian)
```bash
sudo apt install \
    cmake clang \
    libasound2-dev \
    libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
    libxkbcommon-dev libgl1-mesa-dev libwayland-dev
```

### System Packages (Fedora)
```bash
sudo dnf install \
    cmake clang \
    alsa-lib-devel \
    libxcb-devel libxkbcommon-devel mesa-libGL-devel wayland-devel
```

### System Packages (Arch)
```bash
sudo pacman -S cmake clang alsa-lib libxcb libxkbcommon mesa wayland
```

## Runtime Dependencies

| Dependency | Required | Purpose |
|------------|----------|---------|
| [Ollama](https://ollama.ai) | Yes | LLM inference backend |
| PulseAudio/PipeWire | Yes | Audio playback |
| ROCm 6.x | Optional | AMD GPU acceleration |

### Ollama Setup
```bash
# Install
curl -fsSL https://ollama.com/install.sh | sh

# Pull a model
ollama pull llama3.1:8b

# Start server (if not running as service)
ollama serve
```

## Optional: Python ML Tools

For advanced audio benchmarking features (source separation, music generation, effect detection):

```bash
# Create virtual environment
python -m venv ~/.llamaburn-ml
source ~/.llamaburn-ml/bin/activate

# PyTorch - choose ONE:
pip install torch --index-url https://download.pytorch.org/whl/rocm6.0  # AMD ROCm
pip install torch --index-url https://download.pytorch.org/whl/cu121   # NVIDIA CUDA
pip install torch                                                       # CPU only

# Audio Analysis Tools
pip install demucs>=4.0          # Source separation
pip install basic-pitch>=0.3     # Music transcription
pip install audiocraft>=1.0      # MusicGen

# LLM Audio Analysis
pip install transformers>=4.40   # Qwen2-Audio

# Audio Effect Detection (install from git)
# git clone https://github.com/SonyResearch/Fx-Encoder_PlusPlus
# git clone https://github.com/Alec-Wright/OpenAmp
```

## Feature Flags

Build with specific features enabled:

| Feature | Dependencies | Command |
|---------|--------------|---------|
| CPU Whisper | cmake, clang | `cargo build --features whisper` |
| GPU Whisper (ROCm) | cmake, clang, ROCm | `cargo build --features whisper-gpu` |
| Audio Input | libasound2-dev | `cargo build --features audio-input` |
| Full | All above | `cargo build --features whisper-gpu,audio-input` |

## Verification

```bash
# Check Rust toolchain
rustc --version  # Should be 1.92+

# Check Ollama
curl http://localhost:11434/api/tags

# Check Python ML (if installed)
python -c "import demucs; import basic_pitch; print('ML tools OK')"
```
