#!/usr/bin/env python3
"""
OpenAmp audio effect detection wrapper for LlamaBurn.

Usage: python openamp_detect.py <audio_file>

Outputs JSON with detected effects and encodings.
"""
import json
import sys
from pathlib import Path


def main():
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Usage: openamp_detect.py <audio_file>"}))
        sys.exit(1)

    audio_path = Path(sys.argv[1])
    if not audio_path.exists():
        print(json.dumps({"error": f"File not found: {audio_path}"}))
        sys.exit(1)

    try:
        import librosa
        from openamp import FxEncoder

        audio, sr = librosa.load(str(audio_path), sr=None)
        encoder = FxEncoder.from_pretrained()
        result = encoder.extract(audio, sr)

        effects = []
        for name, conf in result.get("detected_effects", {}).items():
            effects.append({"name": name, "confidence": float(conf)})

        output = {
            "effects": effects,
            "embeddings": result.get("encodings", []),
        }
        print(json.dumps(output))

    except ImportError as e:
        print(json.dumps({
            "error": f"Import failed: {e}",
            "install": "pip install openamp librosa"
        }))
        sys.exit(1)
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
