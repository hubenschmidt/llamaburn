#!/usr/bin/env python3
"""
Fx-Encoder++ audio effect detection wrapper for LlamaBurn.

Usage: python fx_encoder_detect.py <audio_file>

Outputs JSON with detected effects and embeddings.
"""
import json
import sys
from pathlib import Path


def main():
    if len(sys.argv) < 2:
        print(json.dumps({"error": "Usage: fx_encoder_detect.py <audio_file>"}))
        sys.exit(1)

    audio_path = Path(sys.argv[1])
    if not audio_path.exists():
        print(json.dumps({"error": f"File not found: {audio_path}"}))
        sys.exit(1)

    try:
        from fx_encoder import FxEncoder

        encoder = FxEncoder.from_pretrained()
        result = encoder.analyze(str(audio_path))

        # Normalize output to standard format
        effects = []
        for name, conf in result.get("effects", {}).items():
            effects.append({"name": name, "confidence": float(conf)})

        output = {
            "effects": effects,
            "embeddings": result.get("embeddings", []),
        }
        print(json.dumps(output))

    except ImportError:
        print(json.dumps({
            "error": "fx_encoder not installed",
            "install": "git clone https://github.com/SonyResearch/Fx-Encoder_PlusPlus && pip install -r requirements.txt"
        }))
        sys.exit(1)
    except Exception as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
