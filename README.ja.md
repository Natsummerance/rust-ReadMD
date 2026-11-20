<p align="center">
 <b>Languages</b>:
 <a href="README.md">简体中文</a> |
 <a href="README.zh-TW.md">繁體中文</a> |
 <a href="README.en.md">English</a> |
 <b>日本語</b>
</p>

<div align="center">
 <img src="assets/icon-256.png" width="88" alt="ReadMD logo">

 # ReadMD (Pure Rust Edition)

 **Rust ネイティブコアを備えたクロスプラットフォーム Markdown リーダー・エディター**

 長文のセマンティックページ分割 · 元ファイルを保つ表示修復 · Office/PDF のページ閲覧と抽出 · 表の編集と出版 · AI レビュー · VS Code / MCP

 [公式サイト](https://rust.readmd.asia) ｜ [V0.0.7 リリースノート](release/release_notes.md) ｜ [開発支援](https://ifdian.net/a/natsummerance)

 [![platform](https://img.shields.io/badge/Windows%2010%2F11%20%7C%20macOS%2013%2B%20%7C%20Linux-blue)](#ダウンロードとインストール)
 [![i18n](https://img.shields.io/badge/languages-46-orange)](docs/i18n-language-reference.md)
 [![license](https://img.shields.io/badge/license-MIT-green)](LICENSE)
 [![release](https://img.shields.io/github/v/release/Natsummerance/rust-ReadMD)](https://github.com/Natsummerance/rust-ReadMD/releases/latest)
</div>

ReadMD は閲覧、編集、形式変換、出版をローカルで行うためのアプリです。Rust コアが本文と保存を管理し、画面にはシステムの WebView を使います。アプリの実行に Python や Microsoft Office のインストールは不要です。大きな組版、OCR、フォント、メディア機能は、プラットフォーム別の管理対象機能パックとして提供し、オフラインでのインポートに対応します。

## こんな用途に向いています

| 状況 | ReadMD の動作 |
| --- | --- |
| 約 8,000 行または 500 KB を超える文書 | セマンティックページ分割で、目次と全文検索をページ間で連動させます |
| 表の区切り線や数式の閉じ記号が欠けている | メモリ上の表示を修復します。元ファイルの変更は明示的な編集・保存時に行います |
| PDF、DOCX、PPTX の内容を先に確認したい | ページを閲覧し、必要に応じて Markdown を抽出します。Office レイアウト表示は読み取り専用です |
| 複雑な表を整理したい | セルの結合・分割、行高・列幅、結合領域の貼り付け、元に戻す操作に対応します |
| 論文、書籍、技術資料を書く | 引用、数式、定理ブロック、出版プリセットを使い、PDF、DOCX、EPUB、HTML、TeX、PPTX に出力します |
| AI で原稿を修正したい | 接続とモデルを選び、変更をブロック単位で確認し、下書きと復元記録を保持します |
| エディターや AI クライアントで作業する | VS Code 拡張と Rust stdio MCP エントリーポイントを利用できます |

## V0.0.7 の更新

- **閲覧してから抽出**：PDF はページ閲覧モードで開きます。DOCX/PPTX に読み取り専用の元レイアウト表示を追加しました。抽出時に失われる書式を報告し、元ファイルと同名の既存 Markdown を保護します。
- **表と出版**：矩形結合、分割、セル属性、行高・列幅、結合領域の貼り付けを追加しました。PDF/DOCX/PPTX 出力は、各形式が対応する範囲で表のレイアウトを保持します。
- **保存と復元**：連続したタブの終了、文字コードの確認、画像・履歴を含む別名保存、レビュー待ち変更の取り消し、バッチ処理の記録、メディア処理の再開を改善しました。
- **共有レビューと描画**：競合する提案は元の本文、現在の本文、提案を比較して手動で結合できます。描画は SVG として保存し、再度開く、元に戻す、下書きの復元に対応します。
- **開発者機能と AI**：複数ファイルの OpenAPI ローカル参照と更新で同じ生成サービスを使います。ソースが変わると古いプレビューは無効になります。チャットのキャンセル、失敗、プライベートモードに共通の履歴保存規則を適用します。 最終修正には、SSE の受信停滞時のキャンセルと、固定した Office 依存パッケージのライセンス通知の検証を含みます。

詳しくは[リリースノート](release/release_notes.md)をご覧ください。以前の内容は[変更履歴](RELEASE_NOTES.md)に残しています。

今回の同一バージョン更新では、**Confluence、Lightfield、Pearl、Chrome、Field** の5種類のネイティブ PPTX デザインを追加しました。本文と表は編集可能なオブジェクトとして出力し、各デザインの背景と対応するアニメーションを保持します。HTML では **出版閲覧、技術文書、長文マガジン** の3種類を選べます。プレビューと単独ファイルで本文、目次、組版、幅広い表の処理を共有します。[テンプレートの説明](rust/readmd-kernel/templates/pptx/README.md)をご覧ください。

## ダウンロードとインストール

デスクトップ、拡張、MCP 設定キットのバージョンは **V0.0.7** です。[すべてのファイルと SHA-256](https://github.com/Natsummerance/rust-ReadMD/releases/tag/V0.0.7)。

| プラットフォーム | ダウンロード | インストール |
| --- | --- | --- |
| Windows 10/11 x64 | [インストーラー](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMDSetup-windows-x64.exe) · [ポータブル ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-windows-x64.zip) | ZIP 全体を展開し、assets とランタイムを保持して `ReadMD.exe` を実行します。WebView2 が必要です |
| macOS 13+ · Apple Silicon | [DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.zip) | `ReadMD.app` を「アプリケーション」に移動します。[Mac インストール手順](release/macos-install.md)を参照してください |
| macOS 13+ · Intel | [DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.zip) | x64 パッケージを選択します。システムの WebKit を使用します |
| Linux x86_64 | [DEB](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.deb) · [tar.gz](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.tar.gz) | GTK3、WebKitGTK 4.1 などが必要です。DEB に依存ライブラリを記載しています |
| VS Code / 互換エディター | [readmd-vscode-0.0.7.vsix](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-vscode-0.0.7.vsix) | デスクトップ版の導入後、「VSIX からインストール」を選びます。[拡張の説明](packages/vscode-extension/README.md)を参照してください |
| MCP クライアント | [readmd-mcp-server-0.0.7.zip](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-mcp-server-0.0.7.zip) | 文書と接続テンプレートです。導入済みデスクトップ実行ファイルの `--mcp` を使います |

[SHA256SUMS.txt](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/SHA256SUMS.txt) でダウンロードを確認できます。旧版の置き換え前にアプリを終了してください。個人文書を移動する必要はありません。

Mac パッケージは**無料の ad hoc 署名を使用し、Developer ID と Apple 公証は使用していません**。初回起動は Gatekeeper によってブロックされる場合があります。[インストール手順](release/macos-install.md)に従い、「システム設定 → プライバシーとセキュリティ」で ReadMD を許可してください。

配布対象は Windows x64、Linux x64、macOS ARM64/Intel、および VSIX と MCP 設定キットです。UOS、Kylin、Deepin などの互換性はシステムライブラリと実機記録に依存します。Windows/Linux ARM64、AppImage、HarmonyOS/OpenHarmony は今回のダウンロード対象に含まれません。

## ページ閲覧と追加機能

デスクトップパッケージには、対応する Office 閲覧ランタイムとオフライン PDFium パックを同梱します。必要に応じてリソースを準備し、インストール、エンジン検査、実際の処理を区別して表示します。Office 表示は同梱の組版エンジンを使うため、複雑な文書では Word/PowerPoint と表示が異なる場合があります。Markdown 抽出では保持できない内容を報告します。

OCR、Typst、出版用フォント、コード実行環境、音声・動画の文字起こしは、利用可能なプラットフォーム用パックに従います。スキャン PDF には OCR と PDFium、文字起こしには対応する FFmpeg、Whisper エンジンとモデルが必要です。**独立したオフライン FFmpeg/Whisper パックは、上表のデスクトップおよび連携パッケージに含まれません**。今回実際に検証したメディアパックは macOS ARM64 用です。他の環境では実際のパック一覧と機能の状態を確認してください。

PDF は閲覧モードで開き、「元の PDF を編集」を選ぶと対応する編集操作が使えます。保存前には復元と元ファイルの競合保護を行います。PDF フォーム、署名、権限、複雑な Office ファイルの往復編集は形式ごとの対応範囲に従います。PPTX の数式は画像として保持し、ソースをノートに残します。

## ローカル作業と AI

- 閲覧、編集、ローカル変換はコンピューター上で実行します。Web 抽出、AI、メールなどの通信は明示的な操作とサービス設定が必要です。
- AI の変更をブロック単位で確認できます。失敗、キャンセル、プライベートチャットは途中の履歴を保存しません。本文の保存、変更の承認、処理の完了を別々に表示します。
- ローカルネットワークで閲覧、編集、提案レビューを共有できます。共有内容はローカルで明示的に保存すると文書ファイルに書き込まれます。
- BibTeX カード、学術ブロック、数式編集、オフライン HTML プレゼンテーション、デスクトップペットを利用できます。ペットのグローバル入力監視は Windows の対応範囲に従います。
- 46 の表示言語を提供し、キーとプレースホルダーを自動検査します。一部の新しい操作メッセージは英語で表示します。

## VS Code と MCP

[VSIX](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-vscode-0.0.7.vsix) を導入すると、ReadMD コマンドでプレビュー、修復、変換、出力、ワークスペース検索、OpenAPI 文書更新を行えます。必要に応じて `readmd.executablePath` にデスクトップ実行ファイルの絶対パスを指定します。インライン AI 補完は既定で無効です。信頼済みワークスペースで有効にできます。[拡張の説明](packages/vscode-extension/README.md)をご覧ください。

MCP は同じ Rust コアを使い、Python は不要です。`command` を実際の実行ファイルの絶対パスに置き換えてください。

```json
{
  "mcpServers": {
    "readmd": {
      "command": "/absolute/path/to/readmd",
      "args": ["--mcp"]
    }
  }
}
```

macOS の通常のパスは `/Applications/ReadMD.app/Contents/MacOS/ReadMD` です。Windows は `ReadMD.exe` のフルパスを使い、JSON 内のバックスラッシュを `\\` と記述します。[MCP の説明](packages/mcp-server/README.md)に他のテンプレートがあります。

## 検証とソース

V0.0.7 は macOS ARM64 で **6 グループ、244 項目のネイティブ検査**を完了しました。終了・復元、6 形式の出力、リリース候補版の ZIP からのコールド起動・再起動、隔離した V0.0.6 セッションの移行を含みます。アーカイブ内の製品バイナリでも 46 言語の保存文字コード操作を検証し、ページ例外は 0 件でした。[Mac 検証記録](docs/reviews/readmd-v007-macos-rc-2026-11-19.md)には最初の WebKit 起動失敗と同じパッケージでの再検証を残しています。これらの検査範囲、他アプリとの互換性、各システムの実機検証は個別に記録します。

Windows x64、Linux x64、macOS ARM64/Intel のリリースビルドと共有品質検査が通過しました。公開 ARM64 ZIP は通常のコールド起動・再起動も確認し、テスト文書のバイト列を保持しました。この製品パッケージの結果と、以前の候補版の 244 項目検査は別に記録します。[公開検証記録](docs/reviews/readmd-v007-published-2026-11-19.md)と[プラットフォーム表](release/platform-matrix.json)をご覧ください。

```sh
cargo build --locked --release --manifest-path rust/Cargo.toml -p readmd-kernel
```

デスクトップ実行ファイルは `rust/target/release/` に生成します。配布にはフロントエンド資産とプラットフォーム用ランタイムも必要です。[tools](tools) と [scripts](scripts) に関連スクリプトがあります。追加のコード実行には各言語の実行環境が必要です。

[製品インデックス](https://rust.readmd.asia/llms.txt) · [製品リファレンス](https://rust.readmd.asia/llms-full.txt) · [不具合報告](https://github.com/Natsummerance/rust-ReadMD/issues) · [MIT ライセンス](LICENSE)
