# WI-5: アプリ内音声 Agent 統合計画

## 方針

現行の WI-5 は、Mobile が別ホストの Agent service URL/token に接続する想定だった。本 WI ではこれを変更し、Agent のセッションと tool loop を Android アプリ内の Rust (`takusu-android`) で動かす。

LLM と TTS の HTTP リクエスト自体は設定された外部 Provider に送る。Planner API は既存どおり `takusu-local` をアプリ内 localhost server として提供する。

```text
React Native
  ├─ 設定 / 会話 / 承認 UI
  ├─ 録音 / TTS 再生
  └─ localhost Agent API
        ↓
takusu-android
  ├─ takusu-local planner API
  ├─ takusu-agent session / transport
  ├─ Sherpa SenseVoice + Hush
  └─ model download coordinator
        ↓
OpenAI-compatible LLM / Cartesia TTS
```

Audio capture/playback is application I/O. They are never registered as LLM tools, and a spoken acknowledgement never approves a planner mutation.

## Scope

### Included

- versioned authenticated Agent transport on the embedded localhost server;
- session creation, turn execution, unresolved approval recovery, and idempotent approval resolution;
- multiple OpenAI-compatible LLM provider configurations;
- provider model discovery through `GET {base_url}/models` and model dropdown selection;
- multiple TTS configurations with a provider dropdown (Cartesia is the first supported provider);
- Android local STT using Sherpa SenseVoice and Hush;
- resumable/background model preparation with Android foreground progress notifications;
- Mobile Agent conversation and approval UI;
- explicit push-to-talk recording and TTS playback;
- migration of the existing unimplemented AddButton tap to the Agent screen.

### Not included

- Anthropic or other non-OpenAI-compatible LLM APIs;
- hotword activation, `VoiceInteractionService`, or always-on microphone;
- automatic approval from voice input;
- background recording;
- persisted conversation or approval operations after Android process death;
- TTS providers other than Cartesia in the first implementation.

## Transport contract

Expose the following authenticated endpoints under `/api/agent/v1`:

```text
GET    /health
GET    /capabilities
POST   /sessions
POST   /sessions/:id/turns
GET    /sessions/:id/approval
POST   /sessions/:id/approvals/:approval_id
DELETE /sessions/:id
```

Every request and response contains `version: 1`. The Mobile client sends an idempotency key for turns and approval decisions. Retrying an approval decision returns the prior `ApprovalResult`; it never executes the stored operation twice.

The Mobile client sends only an approval ID and decision. It never sends a replacement operation payload.

Sessions are scoped to the authenticated user/token. An unresolved approval is held in the owning `AgentSession` and has the existing expiry semantics. If the Android process dies, the session is treated as lost; no pending operation was written before approval, so the client clears the stale session and starts a new one.

## Rust implementation

### Agent

Add `crates/takusu-agent/src/transport.rs` and expose it from `lib.rs`.

Responsibilities:

- axum handlers and versioned serde DTOs;
- bounded session store;
- per-session turn serialization;
- authentication subject checking;
- unresolved approval lookup;
- idempotency and bounded resolved-result retention;
- conversion of `AgentError` to safe API errors without exposing secrets or prompts.

Make `ApprovalRequest`, `ApprovalResult`, and `TurnResult` serializable. Add `takusu-agent`'s `axum` dependency.

### Android host

Extend `crates/takusu-android/src/lib.rs` to:

- depend on `takusu-agent`;
- construct the active Agent configuration at server startup;
- register existing planner tools against the embedded localhost planner client;
- create the OpenAI-compatible LLM client;
- merge the Agent router into the existing planner router;
- expose model-manager status/control through UniFFI.

Do not move planner business logic into Kotlin or TypeScript.

## LLM provider settings

### Mobile model

```ts
interface LlmProviderConfig {
  id: string;
  name: string;
  provider: 'openai' | 'openrouter' | 'custom';
  baseUrl: string;
  selectedModel: string;
  cachedModels: string[];
  modelsFetchedAt?: string;
}
```

Multiple entries are allowed, including multiple entries of the same provider type. Exactly one `activeLlmProviderId` is selected.

All three initial provider types use the OpenAI-compatible adapter. The provider type supplies defaults and display labels; it does not imply support for a non-compatible API.

### Model discovery

Add an authenticated `OpenAIClient::list_models` / provider model-fetching abstraction. Parse the provider's `/models` response, normalize IDs, remove duplicates, and sort them. A custom endpoint that does not expose `/models` may use a manual model ID fallback.

The settings flow is:

1. choose provider type;
2. enter display name, base URL, and API key;
3. fetch models and check connectivity;
4. select one model from a dropdown;
5. save the draft only after successful validation;
6. choose one active provider.

Metadata is stored in AsyncStorage. API keys are stored separately in SecureStore using provider-specific keys such as `takusu.agent.llm.apiKey.<id>`.

## TTS provider settings

```ts
interface TtsProviderConfig {
  id: string;
  name: string;
  provider: 'cartesia';
  voiceId: string;
  model?: string;
  language: string;
  sampleRate: number;
  speed?: number;
}
```

Multiple TTS entries are allowed, with exactly one `activeTtsProviderId`. The UI uses a provider dropdown even though the first release has only Cartesia. TTS keys are stored separately in SecureStore.

The Rust factory must select the active configuration through the existing `TextToSpeech` trait. Future providers add an implementation and enum value without changing the Agent session contract.

## Model downloads

The known local models are:

- Hush denoiser;
- Sherpa SenseVoice int8 Japanese-capable ASR.

Refactor `takusu-audio::ModelCache` to support:

- streaming download to a `.part` file;
- throttled byte progress callbacks;
- pinned model version and SHA-256 verification;
- staging extraction and atomic installation;
- safe archive paths and required-file verification;
- cancellation and retry without leaving a usable partial directory.

On Android, store models under an app-owned persistent/no-backup model directory rather than an evictable cache directory.

Use Android WorkManager unique work per model. The Worker runs as a foreground worker and updates a dedicated `agent-model-downloads` notification channel with download, extraction, verification, completion, retry, and failure states. JS only observes status; it must not own the long-running download.

Expose native status/control to Mobile through the existing server Expo module or a focused Agent Expo module. Notification taps deep-link to `/settings/agent`.

## Mobile files

Add:

```text
mobile/src/api/agentClient.ts
mobile/src/api/agentSettingsStore.ts
mobile/src/api/AgentProvider.tsx
mobile/src/views/AgentSettingsView.tsx
mobile/src/views/AgentView.tsx
mobile/src/components/AgentMessage.tsx
mobile/src/components/AgentComposer.tsx
mobile/src/components/ApprovalSheet.tsx
mobile/src/components/settings/LlmProviderEditor.tsx
mobile/src/components/settings/TtsProviderEditor.tsx
mobile/src/components/settings/ModelDownloadRow.tsx
mobile/app/agent.tsx
```

Update:

```text
mobile/src/views/SettingsView.tsx
mobile/app/settings/[category].tsx
mobile/src/components/AddButton.tsx
mobile/src/views/HomeView.tsx
mobile/src/notifications/channels.ts
mobile/src/notifications/index.ts
```

Settings add an `agent` category. AddButton tap opens `/agent`; upward slide retains the existing task creation gesture.

The Agent screen supports text input first, then push-to-talk. It renders assistant responses, recording/thinking/speaking state, errors, session loss, and approval sheets. Approval sheets show Why, inferred fields, Changes, Warnings, and explicit Deny/Approve controls.

## Mobile audio

Use Android native `AudioRecord` for explicit push-to-talk capture in mono 16 kHz PCM. This avoids requiring Rust to decode a compressed Expo recording format before Sherpa inference.

The native/Rust path is:

```text
AudioRecord PCM
  → f32 conversion
  → Hush denoise
  → Sherpa SenseVoice
  → Agent run_turn
  → active TTS provider
  → WAV/PCM playback
```

Keep audio failures separate from Agent mutation state. A failed recording, STT, TTS, or playback operation must not retry a planner mutation.

## Implementation order

1. Update this plan and shared transport fixtures.
2. Implement serializable Agent contracts, transport, session store, and idempotency.
3. Integrate the Agent router into `takusu-android` and add a text-only Mobile client path.
4. Add LLM model listing and multi-provider settings storage/UI.
5. Add TTS provider settings/factory and Cartesia test playback.
6. Refactor model downloads for progress/integrity and add WorkManager notifications.
7. Add Agent conversation and approval UI, then wire AddButton.
8. Add Android AudioRecord, local Hush/Sherpa transcription, and TTS playback.
9. Run Rust, TypeScript, Kotlin, Android build, and physical-device smoke tests.

## Verification

### Rust

- DTO and version fixture compatibility;
- model-list success, malformed response, unauthorized response, and custom manual fallback;
- session ownership and bounded expiry;
- concurrent turn serialization;
- duplicate turn and approval delivery;
- unresolved approval recovery;
- no write before approval;
- model progress, cancellation, hash mismatch, unsafe archive path, and atomic installation.

### Mobile

- provider add/edit/remove and active selection;
- API keys never persisted in AsyncStorage;
- model dropdown loading/error/manual fallback;
- TTS provider dropdown and test action;
- AddButton tap vs upward slide;
- conversation restart and lost-session handling;
- approval duplicate tap, retry, deny correction, and refresh after approval.

### Android device

- model download while backgrounded and after process recreation;
- notification progress/deep link;
- network interruption and retry;
- storage failure;
- microphone permission denial;
- Japanese STT;
- Cartesia playback;
- spoken acknowledgement cannot approve a planner change.

Required checks:

```text
cargo fmt --check
cargo clippy --workspace
cargo nextest run --workspace
cd mobile && npm run lint
cd mobile && npm run fmt:check
cd mobile && npx tsc --noEmit
cd mobile && npm run kt:lint
Android release APK build
```
