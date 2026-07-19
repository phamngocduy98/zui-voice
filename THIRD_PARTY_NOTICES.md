# Third-Party Notices

## NVIDIA Nemotron 3.5 ASR Streaming 0.6B

The `nemotron-3.5-asr-streaming-0.6b-q8_0.gguf` asset is a Q8_0 conversion of
NVIDIA's Nemotron 3.5 multilingual ASR model.

- Source model: https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b
- Source model release: `nemotron-3.5-asr-streaming-0.6b-v1`, published June 4, 2026
- GGUF conversion: https://huggingface.co/mudler/parakeet-cpp-gguf
- Pinned conversion revision: `bf0af9f425fa01809cadec671b3cb672709d13e9`
- Release artifact SHA-256: `ba2f13eccd4a5245be728f77e6149bd6a4fdcdd133ff2e08ac6005bcef7a99f1`
- Governing model terms: OpenMDW-1.1, reproduced in `licenses/OpenMDW-1.1.txt`
- Conversion publisher: mudler/parakeet-cpp-gguf
- Converter repository metadata: CC-BY-4.0 (https://creativecommons.org/licenses/by/4.0/)

The distributed artifact is a converted and Q8_0-quantized form of the NVIDIA
checkpoint, not the original checkpoint. Its pinned size and SHA-256 are verified
before release and again by Zui. Voice before installation.

Zui. Voice exposes only the 19 transcription-ready and 13 broad-coverage
locales identified by NVIDIA. It deliberately excludes the 8 adaptation-ready
locales, which require fine-tuning. The application records a complete utterance
and transcribes after key release; it does not expose the model's live-streaming
mode.

The model produces probabilistic transcripts. Accuracy depends on language,
accent, audio conditions, and domain. Users must validate it for their use case.

## parakeet.cpp

The `parakeet-server.exe` release asset is based on parakeet.cpp commit
`e8acc6172a94e20a952cf1843decace5d771a94b` (the v0.4.0 tag), with the small
language-forwarding patch in `patches/parakeet-server-language.patch`:
https://github.com/mudler/parakeet.cpp/tree/e8acc6172a94e20a952cf1843decace5d771a94b

MIT License

Copyright (c) 2026 the parakeet.cpp authors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

## Rust and JavaScript dependencies

Source packages remain governed by their respective licenses.
