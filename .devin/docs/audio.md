# Audio (takusu-audio)

## Text-to-Speech

- `TextToSpeech` trait: `synthesize(request) -> Result<Vec<u8>, TtsError>`
- `TtsRequest`, `TtsOptions`, `TtsConfig`, and `TtsError` are shared types
- A new concrete backend will be added alongside `takusu-audio` STT backends
