# Speech models (downloaded on request, not bundled)

NøteCraft does not ship any speech model. The person downloads them from Settings, Meetings, and can delete them there.

- **Whisper** (OpenAI), MIT License. The ggml conversions of whisper.cpp are by Georgi Gerganov, MIT License. Source: https://huggingface.co/ggerganov/whisper.cpp
- **Parakeet TDT 0.6B v3** (NVIDIA), Creative Commons Attribution 4.0 (CC BY 4.0). ONNX conversion and int8 quantization by istupakov. Source: https://huggingface.co/istupakov/parakeet-tdt-0.6b-v3-onnx
  Original model: https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3. Changes: converted to ONNX and quantized to int8.
- **ONNX Runtime** (Microsoft), MIT License, linked into the application to run Parakeet, through the `ort` and `parakeet-rs` crates.
