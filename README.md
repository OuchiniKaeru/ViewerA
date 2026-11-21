# ViewerA

Tauri 2 / Rust製のローカルファイル閲覧アプリです。登録フォルダ内のファイルを閲覧専用で表示し、PDF、画像、テキスト、Markdown、HTML / MHTML、CSV、Office文書、音声・動画、ZIPのプレビューに対応します。ファイル内容検索、お気に入り、最近使った項目も利用できます。Officeファイルの編集やAgentからのコマンド実行は行いません。

## 主な機能

- **プレビュー**: PDF(PDF.js)、画像、DOCX / PPTX / Excel、Markdown、HTML / MHTML、CSV、テキスト / JSON / YAML、音声・動画、ZIP一覧
- **HTML / MHTML**: サンドボックス化されたiframeでレンダリングします。スクリプト実行・フォーム送信・外部ネットワークアクセスは無効で、MHTMLは埋め込みリソースをdata URL化して表示します
- **プレビュー / コード切替**: `.md` / `.html` / `.mhtml` はプレビューとソースコードをツールバーのトグルで切り替えられます
- **カラーテーマ**: Light / Dark に加え、Lite-pink / Light-orange / Light-green / Light-purple / Dark-green / Dark-pink / Dark-orange / Dark-yellow の全10種。設定から選択し、このデバイスに保存されます
- **パネルリサイズ**: サイドバー(160–400px)とプレビューパネル(360–800px)の幅を境界のドラッグで変更できます。キーボードの←→(Shiftで大きく移動)にも対応し、変更は自動保存されます
- **設定**: ツールバー右端のギアアイコンから開きます(テーマ・プレビュー幅・隠しファイル表示)

## 開発

```sh
npm install
npm run tauri dev
```

`npm run tauri dev`はデスクトップウィンドウを起動します。ブラウザーで`http://127.0.0.1:1420`を直接開いた場合、Tauriのネイティブ機能（フォルダー選択など）は利用できません。ブラウザーにはその旨を案内します。

フロントエンドの型チェックと本番ビルド:

```sh
npm run build
```

Rustテスト:

```sh
cargo test --manifest-path src-tauri/Cargo.toml
```

FrontendとAgent Host Clientのテスト:

```sh
npm test
node --test skill/viewera-skill/node/tests/*.test.mjs
python3 -m unittest discover -s skill/viewera-skill/python/tests -v
cargo test --manifest-path skill/viewera-skill/rust/Cargo.toml
```

Node.jsは開発・ビルド時だけ必要です。配布アプリは静的Frontendを内包し、Python、Node.js、LibreOfficeを実行時依存としません。LinuxデスクトップではWebKitGTK等のOS共有ライブラリが必要です。

## ビルド

CLIから使うスタンドアロンバイナリ(Rust本体)のビルド:

```sh
npm run build
cargo build --release --manifest-path src-tauri/Cargo.toml --features tauri/custom-protocol
```

生成物: `src-tauri/target/release/viewera`。引数なしで実行するとデスクトップアプリが起動し、既に起動中のインスタンスがある場合はそちらが前面に出ます(single-instance)。`tauri/custom-protocol`フィーチャーを付けないとdevサーバー(localhost:1420)参照モードになり、単体起動時に画面が読み込めません。

## Linuxへのインストールと更新

Linux x64向けdebをビルドします:

```sh
npm run tauri build -- --bundles deb
```

生成された`src-tauri/target/release/bundle/deb/`内のdebをインストールします:

```sh
sudo apt install "./ViewerA_0.1.0_amd64.deb"
```

新しいdebも同じコマンドでインストールすると更新できます。ユーザー設定はOSのアプリデータディレクトリ(`com.fileviewer.desktop`)に保存されます。異なるバージョン間の移行はリリースごとに確認してください。削除する場合はパッケージマネージャーを使い、ユーザーデータは必要に応じて別途削除します。

`src-tauri/tauri.conf.json`にはAppImage、NSIS、MSIも設定されていますが、Linux環境でWindowsパッケージを生成・検証したことを意味しません。WindowsリリースはWindows CIまたはWindows実機でビルド・署名・インストールテストを行ってください。

## CLI

```sh
viewera                 # デスクトップアプリを起動(起動中なら前面へ)
viewera --version
viewera --help
viewera agent-return --help
```

Agent Host IntegrationのPython / Rust / Node.js / PowerShell例、Receipt、エラー処理は[`skill/viewera-skill/SKILL.md`](./skill/viewera-skill/SKILL.md)を参照してください。

## Agent Return

Agent HostはAgent Return JSONをアプリデータの`agent-inbox/requests`へ書き、CLIへ渡します。Payloadはサイズ、UUID、Action、成果物数、ローカルファイルパスを検証し、キューとReceiptはアプリデータに保存します。未登録フォルダの成果物は、ユーザーがフォルダを選択してRoot登録するまでプレビューできません。

```sh
viewera agent-return \
  --payload "/path/to/agent-inbox/requests/request.json" \
  --receipt "/path/to/agent-inbox/receipts/request.receipt.json"
```

Python向け標準ライブラリClientは[`skill/viewera-skill/python/file_viewer_client.py`](./skill/viewera-skill/python/file_viewer_client.py)、JSON Schemaは[`skill/viewera-skill/schema/agent-return-v1.schema.json`](./skill/viewera-skill/schema/agent-return-v1.schema.json)です。Clientの実行例:

```python
import sys
sys.path.insert(0, "skill/viewera-skill/python")
from file_viewer_client import FileViewerClient

client = FileViewerClient()
result = client.open_files(
    ["/path/to/report.pdf"],
    source_platform="my-agent",
    agent_id="report-agent",
    wait_for_receipt=False,
)
```

`wait_for_receipt=True`はViewer側で成果物を実際にプレビューし、Receiptが`opened`になるまで待機します。未登録Rootの確認中は完了しないため、Agent Hostで待機する場合はタイムアウトを考慮してください。

## 制約と未検証項目

- Office Rendererは読み取り専用のプレビューで、Microsoft Officeとの完全な互換性を保証しません。
- HTML / MHTMLプレビューはスクリプトを実行しないサンドボックス表示です。相対パスの画像・CSSは解決しますが、外部リソースやスクリプト依存のページは完全には再現しません。
- `favorites.json` / `recent.json`にはRoot ID、相対パス、時刻のみを保存します。詳細なファイル情報や内容は永続化しません。
- Custom URL Schemeは導入していません。Agent Returnは限定されたアプリデータ領域とCLI引数で受け渡せるため、URL Schemeが増やすOS登録・入力検証面のリスクを避けています。
- Linux x64のdeb成果物とCLIヘルプは確認できます。Windows MSI / NSIS、AppImage、オフラインのGUI実機確認、Node.js未導入の別環境での実行確認は未実施です。
- Agent Sourceの認証鍵を使う署名検証、OS Credential Store連携、プレビューキャッシュ、永続ログの容量・保持日数制御は未実装です。`maxPreviewCacheBytes`は設定データに残っていますが、現在キャッシュ処理には使われていません。

## ライセンス

MIT License — [LICENSE](./LICENSE) を参照してください。

Copyright (c) 2026 OuchiniKaeru
