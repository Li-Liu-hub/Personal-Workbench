"""Offline Kokoro worker. No networking; input is a JSON file, never shell code."""
import json
import sys
import wave
import time
from pathlib import Path

import numpy as np
import sherpa_onnx


def load_model(root, voice):
    config = sherpa_onnx.OfflineTtsConfig(
        model=sherpa_onnx.OfflineTtsModelConfig(
            kokoro=sherpa_onnx.OfflineTtsKokoroModelConfig(
                model=str(root / "model.int8.onnx"),
                voices=str(root / "voices.bin"),
                tokens=str(root / "tokens.txt"),
                data_dir=str(root / "espeak-ng-data"),
                lexicon=str(root / ("lexicon-gb-en.txt" if voice == 2 else "lexicon-us-en.txt")),
                lang="en" if voice == 2 else "en-us",
            ),
            num_threads=2,
            provider="cpu",
            debug=False,
        ),
        max_num_sentences=1,
    )
    if not config.validate():
        raise RuntimeError("Invalid Kokoro model files")
    return sherpa_onnx.OfflineTts(config)


def generate(tts, request):
    audio = tts.generate(request["text"], sid=request["voice"], speed=request["speed"])
    samples = np.asarray(audio.samples)
    if not samples.size or not np.isfinite(samples).all() or np.max(np.abs(samples)) < 0.0001:
        raise RuntimeError("The model returned empty or silent audio")
    pcm = (np.clip(samples, -1, 1) * 32767).astype("<i2")
    with wave.open(request["output"], "wb") as output:
        output.setnchannels(1)
        output.setsampwidth(2)
        output.setframerate(audio.sample_rate)
        output.writeframes(pcm.tobytes())


def main():
    if sys.argv[1] != "--serve":
        request = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
        generate(load_model(Path(request["model"]), request["voice"]), request)
        return
    root, mailbox = Path(sys.argv[2]), Path(sys.argv[3])
    tts, voice = None, None
    while True:
        pending = mailbox / "request.json"
        if not pending.exists():
            time.sleep(0.01)
            continue
        try:
            request = json.loads(pending.read_text(encoding="utf-8"))
            pending.unlink()
            if tts is None or voice != request["voice"]:
                # Only one accent is resident, bounding model memory.
                tts = None
                tts = load_model(root, request["voice"])
                voice = request["voice"]
            if request.get("action") != "warmup":
                generate(tts, request)
            response = {"ok": True}
        except Exception as error:
            response = {"ok": False, "error": str(error)}
        temporary = mailbox / "response.tmp"
        temporary.write_text(json.dumps(response), encoding="utf-8")
        temporary.replace(mailbox / "response.json")


if __name__ == "__main__":
    main()
