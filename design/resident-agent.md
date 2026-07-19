# 常駐型 planner agent

## 背景

現在の Mobile Agent は、チャット画面に音声入力（STT）と読み上げ（TTS）を追加した体験になっている。内部では planner tool、変更提案、承認フローを持っているが、ユーザーから見ると「質問すると返答する ChatLLM」が中心であり、takusu 固有の価値である日中の実行支援まで閉じていない。

目標は、Agent を独立したチャット画面ではなく、予定を実行する間ずっと利用できる planner assistant にすることである。

```text
予定を把握する
→ 次の行動を提示する
→ 着手・進捗・完了・遅延を受け取る
→ 必要なら変更を提案する
→ 承認後に予定を更新する
```

チャット履歴は詳細確認や自由入力のために残すが、プロダクトの主画面にはしない。

## 用語

「常駐」には異なる段階があるため、区別して扱う。

1. **常時アクセス可能**: アプリ内のどの主要画面からでも Agent ボタンを操作できる。
2. **継続音声セッション**: ユーザーが明示的に開始した後、終了するまで Listen → Act → Speak を繰り返す。
3. **Ambient listening**: アプリ画面外を含め、ユーザーが有効化している間はマイクを継続利用し、呼びかけや対象発話を端末内で検出する。

最終的には 3 を目指す。ただし、1 と 2 を先に完成させ、planner の実行ループが有用であることを確認してから導入する。

## Product principles

### Agent は場所ではなく能力

Agent を開くために専用チャット画面へ移動することを必須にしない。主要画面の右側に円形の常駐ボタンを表示し、短い操作は現在の画面上で完結させる。

- タップ: Agent の compact panel を開く。
- 長押し: 押している間 Listen する。
- 上方向への操作: 既存の task add gesture と競合しないよう、画面と状態に応じて明示的に割り当てる。
- Agent が処理中、読み上げ中、承認待ちの場合は、色・アイコン・アニメーションで状態を示す。
- compact panel から必要な場合だけ full conversation/history view を開く。

ボタンは Home の AddButton を単純に複製するのではなく、アプリ全体で一つの Agent surface として状態を共有する。

### planner state を主役にする

Agent の応答は、原則として自由な assistant message ではなく、planner state と次の action を表示する。

```text
今やること
「レポートを書く」 14:00–15:00

[着手] [完了] [延期] [相談]
```

主な presentation は以下とする。

- current task / next task
- start, pause, progress, complete, delay
- schedule summary
- schedule conflict / overdue alert
- planner change proposal
- focused clarification
- fallback text message

LLM に任意の UI JSON を生成させない。tool result、schedule state、approval request などの型付きデータからクライアントが表示を決める。

### 行動の閉ループを優先する

見た目を音声アシスタントらしくする前に、次の縦切りを完成させる。

```text
「レポート始める」
→ task_start proposal
→ 承認
→ task が in_progress になる
→ Home / widget / Agent button に作業中状態が反映される
→ 「30分やって半分終わった」
→ progress と見積もりを更新する
→ 「終わった」
→ task_complete
→ 次の行動または再スケジュールを提示する
```

active work time は wall-clock time ではなく work session の start / pause から計測する。

### 永続的変更は承認を維持する

音声操作や常駐化によって、既存の approval invariant を弱めない。

- タスク、習慣、スケジュール、永続スキルへの変更は承認前に書き込まない。
- Agent の読み上げに対する曖昧な相槌を承認として扱わない。
- 音声承認を導入する場合も、現在の approval ID、対象変更、明示的な approve / deny 判定を使用する。
- 認識が曖昧な対象、数値、日時は focused clarification を行う。

## Interaction model

### Resident button

主要画面の右端に thumb-reachable な円形ボタンを配置する。キーボード、modal、approval sheet、OS gesture area と重ならない位置へ移動できるようにする。

状態は最低限以下を持つ。

```text
idle
listening
transcribing
thinking
waiting_for_user
waiting_for_approval
speaking
error
```

ボタン操作で状態遷移を中断できるようにする。

- listening 中のタップ: 録音を確定する。
- thinking 中のタップ: compact panel を開く。
- speaking 中のタップ: TTS を停止する。
- waiting_for_approval 中のタップ: approval sheet を開く。
- error 中のタップ: 復旧方法を表示する。

### Compact panel

通常の一往復は現在画面上の sheet / overlay で処理する。

- 認識した発話
- Agent が実行中の action
- 結果または次の選択肢
- 必要な承認

過去ログ、tool details、長い相談、セッション切り替えが必要なときだけ full Agent view へ遷移する。

### Continuous voice session

ユーザーが resident button から明示的に開始した間は、次を繰り返す。

```text
listening
→ transcribing
→ acting
→ speaking
→ listening
```

必要な機能:

- VAD による発話区間検出
- TTS 中の barge-in
- TTS 停止
- 無音 timeout
- 明示的な「終了」
- foreground / background 遷移時の一貫した表示
- text input と voice input の modality 分離

音声入力から始まった turn のみ自動読み上げを行う。テキスト入力やバックグラウンドイベントは、設定や緊急度に応じて通知・表示・TTSを選択する。

## Ambient listening

### Target behavior

opt-in 設定を有効にしている間、Android の foreground service と永続通知を使って Agent を利用可能にする。マイク使用中であること、停止方法、現在の状態を常にユーザーへ示す。

常時Listenは「すべての音声を常にクラウドLLMへ送信する」ことを意味しない。通常時の処理は端末内で閉じる。

```text
AudioRecord
→ local VAD
→ local wake / intention gate
→ local ASR
→ 対象発話だけ Agent turn
→ tool execution / proposal
→ TTS or notification
```

wake word を必須にするか、task-related utterance を分類する wake-word-less mode を許可するかは、実機で false positive、電池消費、端末発熱、使いやすさを測定して決める。初期実装は明示的な呼びかけを推奨する。

### Privacy and safety boundaries

- 初期状態は無効にする。
- 有効化時にマイク継続利用、処理範囲、外部送信条件を説明する。
- マイク使用中は Android の foreground notification とアプリ内表示を消せない形で出す。
- notification と resident button の両方から即時停止できるようにする。
- wake / intention gate より前の raw audio は永続化しない。
- rolling buffer が必要な場合はメモリ上に短時間だけ保持し、対象外発話を破棄する。
- raw audio、transcript、機密情報をログへ記録しない。
- lock screen 中、通話中、他アプリの録音中、battery saver 中の動作を定義する。
- LLM・TTS・ネットワーク障害時も録音状態を曖昧にしない。
- 常時Listenから planner mutation を直接確定しない。

### Android constraints

通常アプリとしての ambient listening は microphone foreground service を前提とする。永続通知、foreground service type、マイク権限、最近の Android における background start 制限へ対応する。OS統合を深める段階で `VoiceInteractionService` の採用可否を検討するが、最初から必須にはしない。

Expo / React Native の component lifecycle に常時録音を所有させない。native Android service が AudioRecord と lifecycle を所有し、Rust は VAD、ASR、Agent session を担当する。JS は状態表示とユーザー操作を購読する。

## Architecture

```text
Mobile UI
├── ResidentAgentButton
├── AgentCompactPanel
├── ApprovalSheet
├── FullAgentView
└── AgentStateProvider
          ↕ native events / commands
Android AgentService
├── AudioRecord lifecycle
├── foreground notification
├── audio focus / interruption handling
└── process and power state handling
          ↕ PCM / state
Rust
├── VAD / denoise / ASR
├── wake or intention gate
├── AgentSession
├── planner / progress tools
└── TTS adapter
```

Agent の状態は `FloatingVoiceButton`、composer、AgentView ごとに重複して持たず、一つの session controller に集約する。UI surface が切り替わっても recording、turn、TTS、approval の所有者は変わらない。

## Event-driven assistance

ユーザー発話だけでなく planner event も Agent の入口にする。

- task の開始時刻
- task の終了予定超過
- deadline 違反の予測
- schedule gap
- 未完了タスクの持ち越し
- schedule 未生成
- 睡眠時間への影響

イベントは直接 LLM turn を起動するとは限らない。決定的に生成できる通知や action はアプリ側で生成し、曖昧な調整・説明・提案が必要な場合だけ Agent を呼ぶ。

```text
「レポートの開始時刻です」
[着手] [10分後] [組み直す]
```

## Rollout

### Phase 1: planner execution loop

- progress storage / API / Agent tools
- start / pause / progress / complete / split
- current task card と quick actions
- Home、widget、Agent UI の状態同期
- structured presentation の最小型

### Phase 2: always available

- 主要画面共通の resident button
- centralized Agent session state
- compact panel
- full Agent view を詳細画面へ変更
- planner event notification と deep link

### Phase 3: continuous voice session

- VAD
- Listen → Act → Speak loop
- barge-in と TTS stop
- modality-aware response
- interruption / timeout / error recovery

### Phase 4: ambient listening

- opt-in microphone foreground service
- persistent notification と即時停止
- local wake / intention gate
- privacy・電池・発熱・false positive の実機評価
- background / lock screen lifecycle
- 必要に応じて VoiceInteractionService を評価

## Non-goals

- UIへ波形を追加するだけで問題を解決したことにしない。
- Chat bubble の見た目だけを変えない。
- planner lifecycle がない状態で hotword 対応を先行しない。
- raw audio を常時クラウドへ送らない。
- LLM に任意の UI や承認対象を生成させない。
- 常時Listenをデフォルトで有効にしない。

## Success criteria

- ユーザーが full Agent view を開かずに着手・進捗・完了・延期を行える。
- Agent の結果が Home、schedule、widget に即時反映される。
- 一つの明示的な音声セッション内で複数 turn を継続できる。
- ambient listening の開始・稼働・停止が常に視認できる。
- 対象外音声が外部送信・永続化されない。
- planner mutation はすべて既存の承認境界を維持する。
