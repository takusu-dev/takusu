# Home(task) view

- 上段
    - ハンバーガーメニューボタン(task view以外でも常に表示)
        - コンテキストメニュー
    - searchボタン
    - 同期を実行ボタン(task view以外でも常に表示)
- 中段(一番広い)
    - タスクのカードを並べる
- 下段
    - タスク追加ボタン(タスクビュー以外でも常に表示)
        - まんなか
    - タスク実行開始&Doneボタン
        - 右端

## 追加のUIエレメント

- 画面の右側面の真ん中よりちょっと上にナビゲーションを助けるためのボタン
- 画面の左側面下側にビュー切り替えボタン

## 上段について

### ハンバーガーメニュー

コンテキストメニュー

- 常にあるやつ
    - 設定
    - undo/redo
        - 50stepくらい?
- タスクを選択(後述)することで生えるやつ
    - 選択以外をreschedule
    - 選択をreschedule
    - 削除
    - それらを依存とする新規タスクの作成
    - 選択解除

### タスクの検索

タスクの検索。押すと入力を開始できる

### 同期ボタン

- rescheduleとworker/google calendarへの同期を同時に実行するか、2段階で(最初におしたらresche、pendingがないならpush)は設定で
同期の状態を色で表示

## 中段について

- タスクのカードを時系列順に並べる。pendingのやつは一番上にならべる。
- pendingとschedule済みにあるやつの間、schedule済みのやつ同士でも日付が違うやつの間にちいさくて主張がよわいけどはっきりある横バー
- タスクのカードを長押しすると選択モードへ
    - そのなかではタップして選択/選択解除
- task cardを左から右にスライドしてdoneにできる。そのタイミングで弱めのhaptics
    - 逆にスライドしてdeleteできるけど途中でちょっとでかめのhaptics
- parallelの場合は受け皿のタスクを左に表示(1:3くらいの横幅で)それらも同様にslideできる(両方完了したりdone)
    - それを選択からコンテキストメニューできりはなせるように
- 長押ししたまま動かしてタスクの前後関係を調整できるように
- doneなタスクはタイトルに取消線&灰色
- タスクをタップでtask detail viewへ

(過ぎた日のタスク)
--- <- デフォルトのページ上端。強めに下に引っぱると過ぎた日を見れる
pending
---
(今日の)
---
(明日の)
...

みたいな感じ

### タスクのカード

左側に上下で開始/終了時刻
真ん中にタイトル
abandonなんちゃらで背景色を色を変える
コストを右下に(avg, sigma)

## 下段について

### 追加ボタン

タップでAssistant(あとで), 下から上にスライドでタスクの追加画面を開く

## 追加のUIエレメントについて

### navigation button

rikkahubのやつからのインスパイア。縦に五つのボタンがある

上から
- << を上に向けたみたいなボタン
    - dayごとにスクロール
- < を(以下同文)
    - page(見えてる範囲)ごとにスクロール
- >>
- >
- カレンダーを開くボタン
    - 任意の日を開くため
    - 押すとカレンダーのoverlayみたいなのがでてくる

### view changer

- habit
- task
- graph

でビューを変えれる。縦に並べて省スペースだけど押し易い感じで

# graphビューについて

タスクの依存関係を有向グラフで見れる。未完了のタスクとその依存(完了していても。完了だったらノードを灰色で表示)のタスクを表示する。

taskをtapしてtask detail view
タイトルはなるべく常に表示(引きなら表示できないのはしょうがない)

上段に編集開始ボタンがあって指で依存を切ったり(edgeを指で切ると切断)依存を追加したり(nodeからnodeまで指を動かす)

# habitビューについて

habitカード(タイトルとか周期とか, cost, 情報を表示)を表示する(selectable, context
menuもselectで変わる)
またhabitを追加ボタンもある

# task detail view

それぞれのエレメントを押して編集できる

上から
- title
- {time} -> {time}
    - pendingじゃなければ
- parallel task
    - parallelなら。tap to task detail view
- cost(avg, sigma)
- abandonなんちゃら
    - 5段階で調整。スライドで
    - 真ん中が0.5
- habit
    - もしhabitから生成されたタスクならhabitを押してhabit detail viewへ移動
- description
- parallel config
- deps graph
    - 関係があるやつだけ

# task add view

task detail viewからdesp graphを取って、追加できるようにする画面を生やす

# habit detail view

情報を並べるのと、下の方にこのhabitから生成された直近のタスクのリスト

# habit add view

# 設定

一枚のviewに全部あるわけじゃなくて、カテゴリごとに設定のページがある感じで

- general
    - dark/white theme
    - syncのやつ
- worker
    - endpoint
    - key
- google calendar config
- info
    - license
    - version(build number)

# デザイン

- task, graph, habit以外のviewは一つ前のビューに戻るボタンがいる
- シンプルよりのデザイン。あとでフィードバックする
- #7261A3 がブランドカラーだからアクセントとして使って

