<p align="center">
 <b>Languages</b>:
 <a href="README.md">简体中文</a> |
 <a href="README.zh-TW.md">繁體中文</a> |
 <b>English</b> |
 <a href="README.ja.md">日本語</a>
</p>

<div align="center">
 <img src="assets/icon-256.png" width="88" alt="ReadMD logo">

 # ReadMD (Pure Rust Edition)

 **A cross-platform Markdown reader and editor with a native Rust core**

 Semantic pagination · Non-destructive display repair · Office/PDF reading and extraction · Table editing and publishing · AI review · VS Code / MCP

 [Website](https://rust.readmd.asia) ｜ [V0.0.7 release notes](release/release_notes.md) ｜ [Sponsor](https://ifdian.net/a/natsummerance)

 [![platform](https://img.shields.io/badge/Windows%2010%2F11%20%7C%20macOS%2013%2B%20%7C%20Linux-blue)](#downloads-and-installation)
 [![i18n](https://img.shields.io/badge/languages-46-orange)](docs/i18n-language-reference.md)
 [![license](https://img.shields.io/badge/license-MIT-green)](LICENSE)
 [![release](https://img.shields.io/github/v/release/Natsummerance/rust-ReadMD)](https://github.com/Natsummerance/rust-ReadMD/releases/latest)
</div>

ReadMD brings reading, editing, conversion and publishing into one local workflow. Its Rust core manages documents and saves, while desktop windows use the system WebView. Running the app requires neither Python nor Microsoft Office. Larger typesetting, OCR, font and media components are managed capability packs with platform-specific availability and offline import.

## Is it right for you?

| Your situation | What ReadMD does |
| --- | --- |
| A file exceeds roughly 8,000 lines or 500 KB | Semantic pagination keeps the outline and full-document search connected across pages |
| A table lacks separators or a formula is unclosed | Repairs the display in memory; source files change when you explicitly edit and save |
| You want to inspect a PDF, DOCX or PPTX | Read its pages first, then extract Markdown when needed; Office layout views are read-only |
| You need more control over tables | Merge/split cells, adjust row/column sizes, paste merged regions and undo changes |
| You write papers, books or technical documents | Use citations, formulas, theorem blocks and publishing presets; export PDF, DOCX, EPUB, HTML, TeX and PPTX |
| You want AI help with a draft | Choose a connection and model, review changes in blocks, and retain drafts and recovery records |
| You work in an editor or AI client | Connect through the VS Code extension or Rust stdio MCP entry point |

## What's new in V0.0.7

- **Read before extracting**: PDFs open in page-reading mode. DOCX/PPTX gain read-only original-layout views. Extraction reports format losses and protects source files and existing Markdown files with the same name.
- **Tables and publishing**: Rectangular merges, splitting, cell properties, row/column sizing and merged-region paste. PDF/DOCX/PPTX exports retain table layout within each format's supported scope.
- **Saving and recovery**: Improved consecutive tab closing, encoding confirmation, Save As with images and history, withdrawal of pending edits, batch receipts and media task resumption.
- **Shared review and drawing**: Resolve conflicting suggestions against the original, current and proposed text. Save drawings as SVG with reopening, undo and recovery.
- **Developer tools and AI**: Multi-file OpenAPI references and updates share one local generator; source changes invalidate old previews. Chat cancellation, failure and private mode share consistent history rules. Final fixes cover cancellation under SSE downstream backpressure and the completeness of pinned Office dependency notices.

See the [release notes](release/release_notes.md). Earlier versions remain in the [changelog](RELEASE_NOTES.md).

This same-version update adds five native PPTX designs: **Confluence, Lightfield, Pearl, Chrome and Field**, with editable text and tables and each design’s native artwork and supported animations. HTML offers **Publication Reading, Technical Docs and Longform Magazine** layouts; previews and standalone files share article content, navigation, typography and wide-table handling. See the [embedded design documentation](rust/readmd-kernel/templates/pptx/README.md).

All 10 desktop/integration assets in this refresh are built from [`43905601f2a0ae747f4000aad8e69a80e955ce7c`](https://github.com/Natsummerance/rust-ReadMD/tree/43905601f2a0ae747f4000aad8e69a80e955ce7c). [Matching source ZIP](https://github.com/Natsummerance/rust-ReadMD/archive/43905601f2a0ae747f4000aad8e69a80e955ce7c.zip) · [Release CI](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44669807015). The original `V0.0.7` tag stays at its first-publication commit; use these fixed links for this refresh’s source.

## Downloads and installation

Desktop packages, the extension and the MCP configuration kit use **V0.0.7**. [All assets and SHA-256 checksums](https://github.com/Natsummerance/rust-ReadMD/releases/tag/V0.0.7).

| Platform | Download | Installation |
| --- | --- | --- |
| Windows 10/11 x64 | [Installer](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMDSetup-windows-x64.exe) · [Portable ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-windows-x64.zip) | Extract the entire portable ZIP, keeping assets and runtime libraries, then run `ReadMD.exe`; requires WebView2 |
| macOS 13+ · Apple Silicon | [DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.zip) | Move `ReadMD.app` to Applications; see the [Mac installation guide](release/macos-install.md) |
| macOS 13+ · Intel | [DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.zip) | Choose the x64 package; uses system WebKit |
| Linux x86_64 | [DEB](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.deb) · [tar.gz](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.tar.gz) | Requires GTK3, WebKitGTK 4.1 and other system libraries; the DEB declares its dependencies |
| VS Code / compatible editors | [readmd-vscode-0.0.7.vsix](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-vscode-0.0.7.vsix) | Install the desktop app, then choose “Install from VSIX…”; see the [extension guide](packages/vscode-extension/README.md) |
| MCP clients | [readmd-mcp-server-0.0.7.zip](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-mcp-server-0.0.7.zip) | Documentation and connection templates; uses the installed desktop executable with `--mcp` |

Verify downloads against [SHA256SUMS.txt](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/SHA256SUMS.txt). Quit an older copy before replacing it; your documents can stay where they are.

Mac packages use **free ad hoc signatures, without Developer ID or Apple notarization**. Gatekeeper may block the first launch. Follow the [installation guide](release/macos-install.md) to allow ReadMD in System Settings → Privacy & Security.

Release targets are Windows x64, Linux x64 and macOS ARM64/Intel, plus VSIX and the MCP configuration kit. Compatibility with UOS, Kylin, Deepin and other distributions depends on their libraries and platform evidence. Windows/Linux ARM64, AppImage and HarmonyOS/OpenHarmony are outside this release's download assets.

## Page reading and optional capabilities

Desktop bundles include the matching Office reading runtime and an offline PDFium pack. ReadMD prepares these resources when needed and distinguishes installation, engine probes and actual operations. Office layout views use the bundled layout engine; complex documents may differ from Word or PowerPoint. Markdown extraction reports content it cannot preserve.

OCR, Typst, publishing fonts, code runtimes and transcription depend on the available platform packs. Scanned PDFs require OCR and PDFium. Transcription requires matching FFmpeg, Whisper and model resources. **The independent offline FFmpeg/Whisper packs are separate from the desktop and integration downloads above.** The media packs exercised in this cycle were macOS ARM64; availability elsewhere follows the actual catalog and capability status.

PDFs open for reading; “Edit original PDF” enables supported editing operations with recovery and source-conflict protection before saving. PDF forms, signatures, permissions and complex Office round trips follow their format-specific support. PPTX formulas use images with source text retained in notes.

## Local work and AI

- Reading, editing and local conversion run on your computer. Web extraction, AI, email and other network operations require an explicit action and service configuration.
- Review AI edits in blocks. Failed, cancelled and private chats do not persist partial chat history. Saving a document, accepting edits and completing a task have separate status.
- Share on a local network for reading, editing and suggestions. Shared content reaches the document file when you explicitly save locally.
- Use BibTeX cards, academic blocks, formula editing, offline HTML presentations and desktop pets. Global pet keyboard/mouse monitoring follows Windows support.
- Choose from 46 interface languages. Keys and placeholders have automated checks; some newer workflow messages remain in English.

## VS Code and MCP

Install the [VSIX](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-vscode-0.0.7.vsix), then use ReadMD commands for preview, repair, conversion, export, workspace search and OpenAPI updates. Set `readmd.executablePath` to the desktop executable's absolute path if needed. Inline AI completion is off by default and can be enabled in trusted workspaces. See the [extension documentation](packages/vscode-extension/README.md).

MCP uses the same Rust core and requires no Python. Replace `command` with the installed executable's absolute path:

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

On macOS the usual path is `/Applications/ReadMD.app/Contents/MacOS/ReadMD`. On Windows use the full path to `ReadMD.exe`, escaping backslashes as `\\` in JSON. See the [MCP guide](packages/mcp-server/README.md) for more templates.

## Validation and source

For the 2026-11-20 refresh, the official Apple Silicon ZIP completed two native cold launches/reopenings and was installed locally. Windows/Linux coverage uses the actual release binaries in CI lifecycle and image-export regressions; Intel coverage uses Mac CI build, signing and packaging checks.

The 2026-11-19 V0.0.7 release candidate completed **244 native checks in six groups on macOS ARM64**: closing and recovery, six export formats, cold launch/reopening from its packaged ZIP, and migration of an isolated V0.0.6 session. The archived production binary also completed the 46-language save-encoding flow with zero page exceptions. The [Mac report](docs/reviews/readmd-v007-macos-rc-2026-11-19.md) retains an initial WebKit cold-launch failure and successful same-package reruns. These checks have a defined scope; application interoperability and physical coverage on each system are recorded separately.

For the initial 2026-11-19 publication, release builds and shared quality gates passed for Windows x64, Linux x64 and macOS ARM64/Intel. The published ARM64 ZIP also completed normal cold launch and reopening with the test document unchanged. This final-package evidence is separate from the earlier 244-check candidate. See the [publication report](docs/reviews/readmd-v007-published-2026-11-19.md) and [platform matrix](release/platform-matrix.json).

```sh
cargo build --locked --release --manifest-path rust/Cargo.toml -p readmd-kernel
```

The desktop binary is under `rust/target/release/`. Distribution also requires frontend assets and platform runtimes; scripts are in [tools](tools) and [scripts](scripts). Optional code execution requires its language runtime.

[Product index](https://rust.readmd.asia/llms.txt) · [Full product reference](https://rust.readmd.asia/llms-full.txt) · [Issues](https://github.com/Natsummerance/rust-ReadMD/issues) · [MIT license](LICENSE)
