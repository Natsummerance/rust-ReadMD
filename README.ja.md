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

 **Rust ネイティブコアを備えたクロスプラットフォーム Markdown リーダー＆エディター**

 長大文書の意味的ページネーション · 表示層での非破壊修復 · Office/PDF/Web から MD への変換 · オフライン OCR · LaTeX 学术支援 · MCP 連携 · Python 依存性ゼロ

 公式サイト：[https://rust.readmd.asia](https://rust.readmd.asia) ｜ 開発支援（Afdian）：[https://ifdian.net/a/natsummerance](https://ifdian.net/a/natsummerance)

> [!IMPORTANT]
> **Rust ネイティブ版（`rust-ReadMD`）**：閲覧、編集、保存、形式変換を Rust コアが管理し、Python や Office のインストールは不要です。デスクトップ画面にはシステムの WebView を使用し、大規模な組版や OCR はオフラインで導入できる管理対象の機能パックとして提供します。現在の主な実機検証環境は Windows です。他のプラットフォームではビルド成果物とネイティブ動作の検証を区別して記録します。詳細は `release/platform-matrix.json` を参照してください。

 [![platform](https://img.shields.io/badge/Windows%20%7C%20macOS%20%7C%20Linux%20%7C%20KylinOS%20%7C%20UOS-blue)](#プラットフォーム別ダウンロード-release-assets)
 [![i18n](https://img.shields.io/badge/languages-46-orange)](docs/i18n-language-reference.md)
 [![license](https://img.shields.io/badge/license-MIT-green)](LICENSE)
 [![release](https://img.shields.io/github/v/release/Natsummerance/rust-ReadMD)](https://github.com/Natsummerance/rust-ReadMD/releases/latest)
 [![website](https://img.shields.io/badge/site-rust.readmd.asia-black)](https://rust.readmd.asia)
</div>

## こんな用途に向いています

| 状況 | ReadMD の動作 |
| --- | --- |
| 8,000 行または 500KB を超える文件 | セマンティックページ分割で目次と Ctrl+F 検索を使いやすく保ちます |
| 表の区切り線や数式が壊れている場合 | メモリ上の表示だけを修復し、元ファイルは変更しません |
| DOCX / PPTX / XLSX / PDF / HTML がある | Markdown への変換で手作業を減らします |
| 画像内の文字が必要 | 対応する OCR 機能パックを必要に応じて導入し、オフラインで認識できます |
| 論文や技術資料を書く場合 | BibTeX カード、定理・証明ブロック、LaTeX エクスポートを支援します |
| デスクトップでペットと対話したい | CC0 Arch-Chan Live2D & 73種の呼吸スプライト、全画面フキダシ同期 |
| PDF を編集したい | 対応プラットフォームに PDFium 機能パックを導入し、利用可能な操作を機能の状態表示で確認できます |
| AI ワークフローに接したい | VS Code 拡張と Rust stdio MCP サーバーを提供します |
| 自分の言語で操作したい | 46 の表示言語を提供し、キー・プレースホルダー・長文のレイアウトを検査しています |

## 直接ダウンロード

[Windows インストーラー](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMDSetup-windows-x64.exe) ·
[Windows ポータブル版](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-windows-x64.zip) ·
[macOS Apple Silicon](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-arm64.zip) ·
[macOS Intel](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-x64.zip) ·
[Linux tar.gz](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.tar.gz) ·
[UOS / 麒麟 Deb](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.deb) ·
[VS Code 拡張](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.5/readmd-vscode-0.0.5.vsix) ·
[MCP 設定キット](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.5/readmd-mcp-server-0.0.5.zip) ·
[SHA-256](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/SHA256SUMS.txt)

## 3 ステップで開始

1. 環境に合うインストーラーまたはポータブル版を入手します。
2. 実際に大きいファイルを開き、目次・検索・表表示を確認します。
3. 編集は保存したいときだけ行います。表示修復は勝手に元ファイルへ書き込みません。

## デスクトップ伴走ペットシステム (Live2D & 73種のスプライト)

- **CC0 Arch-Chan Live2D**: 読書進捗や時間帯に連動する高精細 Live2D ボーンアニメーション。ドラッグ、マウス透過、高感度クリック判定に対応。
- **73種の Petdex スプライトと微呼呼吸エンジン**: 幾何自動推論アルゴリズムにより高密度連番や単体立ち絵を遅延なく自動認識。足元アンカーの正弦波微呼吸により生命感あふれる動きを実現。
- **伴走ライフステータスエンジン**: レベル、体力、機嫌、親密度の永続成長システム。頭をなでる、食事、遊び、休憩、起床などのアクションに対応。
- **全画面フキダシ同期**: デスクトップ浮動ウィンドウ、アプリ内ウィジェット、設定ステージの3箇所で台詞フキダシをリアルタイム同期。Electron 右クリックメニューとも連携。
- **Apple 風高品位スライダー (/apple-design)**: Apple HIG に準拠した 4px 極細トラック、18px 物理質感フローティングノブ、Active 触覚グロー、等幅数字バッジ表示。

---

## ネイティブ高精度ベクター PDF エディター

- **構造および微視的監査**: PDF の幾何寸法、DPI、回転、ファイルロックを自動監査し、指定領域の背景色とノイズをサンプリング。
- **ゼロ汚染差分検査 (Zero-Contamination Gate)**: サンドボックス内でシミュレーション編集を行い、比較画像とヒートマップを自動生成。監査を通過しない限り元ファイルを変更しません。
- **安全な永続化と一発ロールバック**: 保存前に `.bak` 物理バックアップを強制生成し、Windows の読み取り専用属性を自動解除。ワンクリックで元状態へ復元可能。

---

## AI アシスタント引用リソース

- [製品インデックス](https://rust.readmd.asia/llms.txt): バージョン、対応環境、プライバシー境界、主要な事実。
- [完全引用コーパス](https://rust.readmd.asia/llms-full.txt): 長文の改ページ、非破壊修復、変換、よくある質問への直接回答。

## Star が役に立つ理由

ReadMD は、長期間保管する資料で起こりやすい問題に取り組みます。大きなファイルも読み続けられ、取り込んだ資料の整理作業が減り、機密性の高い草稿はローカルに残せます。Windows、macOS、Linux、中国 OS の間でも同じ操作感を維持できます。

もし整理時間の短縮に役立ったら、[リポジトリに Star](https://github.com/Natsummerance/rust-ReadMD) を付けて他の執筆者にも見つけてもらいましょう。


## プラットフォーム別ダウンロード (Release Assets)

配布ファイルがあることは、すべての環境でネイティブ検証が完了したことを意味しません。対象範囲と検証記録は `release/platform-matrix.json` を参照してください。Windows 7/8、HarmonyOS/OpenHarmony、未公開のアーキテクチャは V0.0.5 の配布対象外です。

| プラットフォーム | アーキテクチャ / 形式 | 直接ダウンロードリンク (GitHub Release) | 概要 |
| :--- | :--- | :--- | :--- |
| **Windows** | x64 (インストーラー) | [⬇️ **ReadMDSetup-windows-x64.exe**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMDSetup-windows-x64.exe) | Windows x64 インストーラー。既定のアプリはシステム設定で選択 |
| **Windows** | x64 (ポータブル版) | [⬇️ **ReadMD-windows-x64.zip**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-windows-x64.zip) | ZIP 全体を展開し、assets とランタイムを保持したまま ReadMD.exe を実行 |
| **macOS** | Apple Silicon (M1〜M4) | [⬇️ **ReadMD-macos-arm64.zip**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-arm64.zip) | Apple Silicon Mac アプリ。ネイティブ検証状況はプラットフォーム表を参照 |
| **macOS** | Intel x86_64 | [⬇️ **ReadMD-macos-x64.zip**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-x64.zip) | Intel Mac アプリ。ネイティブ検証状況はプラットフォーム表を参照 |
| **Linux** | x86_64 (tar.gz) | [⬇️ **ReadMD-linux-x86_64.tar.gz**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.tar.gz) | 展開後に ReadMD を実行。システムの GTK / WebKitGTK が必要 |
| **Linux / 国産 OS** | Debian / Ubuntu / UOS / 麒麟 | [⬇️ **ReadMD-linux-x86_64.deb**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.deb) | x86_64 Deb パッケージ。各環境の互換性はネイティブ検証記録を参照 |
| **チェックサム** | SHA-256 リスト | [⬇️ **SHA256SUMS.txt**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/SHA256SUMS.txt) | 配布ファイルの整合性検証用チェックサム |

Windows ZIP は全体を展開し、ディレクトリ構成を保持してください。デスクトップの起動には Windows WebView2、macOS のシステム WebKit、または Linux GTK / WebKitGTK が必要です。OCR、PDFium、Typst、出版用フォントは任意の機能パックで、基本インストーラーや ZIP には含まれません。エンジンパックの準備・実機検証は現在 Windows x64 のみで、他の環境は実際のパック一覧を確認してください。Linux ARM64 / 麒麟 ARM64 は現在ビルドスクリプトのみで、V0.0.5 インストーラーは未公開です。

---

<div align="center">

**ReadMD** · ローカル優先、検証済みの対応環境で使える Markdown ツール。

</div>
