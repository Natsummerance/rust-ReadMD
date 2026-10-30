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

 **Cross-platform Markdown reader and editor with a native Rust core**

 Semantic pagination for massive files · Non-destructive display repair · Office/PDF/Web to MD · Offline OCR · LaTeX academic suite · MCP agent integration · Zero Python dependencies

 Official Site: [https://rust.readmd.asia](https://rust.readmd.asia) ｜ Sponsor via Afdian: [https://ifdian.net/a/natsummerance](https://ifdian.net/a/natsummerance)

> [!IMPORTANT]
> **Rust Native Edition (`rust-ReadMD`)**: The Rust kernel manages reading, editing, saving and format adapters. Running ReadMD requires neither Python nor Office. Desktop windows use the system WebView; larger typesetting and OCR engines are managed capability packs that support offline import. Windows is the primary tested platform. Build artifacts and full native validation for other platforms are recorded separately in `release/platform-matrix.json`.

 [![platform](https://img.shields.io/badge/Windows%20%7C%20macOS%20%7C%20Linux%20%7C%20KylinOS%20%7C%20UOS-blue)](#direct-downloads-platforms-matrix-release-assets)
 [![i18n](https://img.shields.io/badge/languages-46-orange)](docs/i18n-language-reference.md)
 [![license](https://img.shields.io/badge/license-MIT-green)](LICENSE)
 [![release](https://img.shields.io/github/v/release/Natsummerance/rust-ReadMD)](https://github.com/Natsummerance/rust-ReadMD/releases/latest)
 [![website](https://img.shields.io/badge/site-rust.readmd.asia-black)](https://rust.readmd.asia)
</div>

## Is it right for you?

| Your situation | What ReadMD does |
| --- | --- |
| A file exceeds 8,000 lines or 500 KB | Semantic pagination keeps the outline and Ctrl+F search usable |
| A table lacks separators or math is unclosed | Rendering repairs the display in memory while the original stays unchanged |
| You have DOCX / PPTX / XLSX / PDF / HTML | Convert it to Markdown instead of rebuilding formatting by hand |
| An image contains text you need | Install the matching OCR capability pack for offline recognition where available |
| You write papers or technical documents | Use BibTeX cards, theorem/proof boxes and LaTeX export |
| You want desktop companion & interaction | CC0 Arch-Chan Live2D & 73 micro-breathing sprites with full-stack bubble sync |
| You need PDF editing | Install the PDFium capability pack on a supported platform; capability status describes the available operations |
| You want an AI-assisted workflow | Use the VS Code extension and Rust stdio MCP server |
| You prefer your own interface language | Choose from 46 interface languages, with key, placeholder and long-text layout checks |

## Direct downloads

[Windows Setup](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMDSetup-windows-x64.exe) ·
[Windows Portable](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-windows-x64.zip) ·
[macOS Apple Silicon](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-arm64.zip) ·
[macOS Intel](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-x64.zip) ·
[Linux tar.gz](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.tar.gz) ·
[Deb for UOS/Kylin](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.deb) ·
[VS Code extension](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.5/readmd-vscode-0.0.6.vsix) ·
[MCP configuration kit](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.5/readmd-mcp-server-0.0.6.zip) ·
[SHA-256](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/SHA256SUMS.txt)

## Start in three steps

1. Download the installer or portable build for your system.
2. Open a real large file and check outline, search and table rendering.
3. Edit only when you are ready; rendering repairs do not silently write to your source.

## Desktop Companion Pet System (Live2D & 73 Breathing Sprites)

- **CC0 Arch-Chan Live2D**: High-definition Live2D skeletal animation that dynamically reacts to reading progress and time of day, featuring smooth drag, mouse pass-through, and high-sensitivity hit testing.
- **73 Petdex Sprites with Micro-Breathing Engine**: Automatic geometry inference resolves dense spritesheets and standalone icons with zero latency; bottom-anchor sinusoidal micro-breathing brings static sprites to life.
- **Companion Life State Engine**: Persistent growth system with Level, Energy, Mood, and Affection, supporting petting, feeding, playing, resting, and waking interactions.
- **Full-Stack Bubble & Context Menu Sync**: Floating desktop window, in-app widget, and settings stage keep dialogue bubbles synchronized in real-time, integrated with the Electron tray menu.
- **Tactile Apple-Style Slider (/apple-design)**: Apple HIG 4px slim track with an 18px physical tactile knob, active energy glow, and tabular numeral pill readouts.

---

## Native Vector PDF Editor

- **Structural & Microscopic Audit**: Automatically inspects PDF geometry, DPI, rotation, file locks, and samples background color/noise in specified regions.
- **Zero-Contamination Gate**: Simulates edits in a sandbox and generates differential comparison images and heatmaps; strictly prevents writing to the original file unless verified.
- **Safe Persistence & One-Click Rollback**: Automatically creates physical `.bak` backups and unlocks Windows read-only flags; supports instant rollback to the original file.

---

## AI assistant citations

- [Concise product index](https://rust.readmd.asia/llms.txt): version, platforms, privacy boundary, and key facts.
- [Full citation corpus](https://rust.readmd.asia/llms-full.txt): direct answers about long-document pagination, non-destructive repair, conversion, and frequent questions.

## Why star ReadMD?

ReadMD solves the unglamorous problems in a long-lived document library: large files remain readable, imported material needs less cleanup, sensitive drafts stay local, and the original file retains final authority. If it saves you one cleanup session, please [star the repository](https://github.com/Natsummerance/rust-ReadMD) so other writers can find it.


## Official Downloads & Platforms Matrix (Release Assets)

Download assets do not imply completed native validation on every platform; see `release/platform-matrix.json` for scope and evidence. Windows 7/8, HarmonyOS/OpenHarmony and unpublished architectures are outside the V0.0.5 release scope.

| OS / Platform | Architecture / Format | Direct Download Link (GitHub Release) | Description |
| :--- | :--- | :--- | :--- |
| **Windows** | x64 (Installer) | [⬇️ **ReadMDSetup-windows-x64.exe**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMDSetup-windows-x64.exe) | Windows x64 installer; select default apps in system settings |
| **Windows** | x64 (Portable) | [⬇️ **ReadMD-windows-x64.zip**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-windows-x64.zip) | Extract the complete ZIP, retaining assets and runtime libraries, then run ReadMD.exe |
| **macOS** | Apple Silicon (M-Series) | [⬇️ **ReadMD-macos-arm64.zip**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-arm64.zip) | Apple Silicon Mac app package; see the platform matrix for native validation |
| **macOS** | Intel x86_64 | [⬇️ **ReadMD-macos-x64.zip**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-x64.zip) | Intel Mac app package; see the platform matrix for native validation |
| **Linux** | x86_64 (tar.gz) | [⬇️ **ReadMD-linux-x86_64.tar.gz**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.tar.gz) | Extract and run ReadMD; requires system GTK / WebKitGTK |
| **Domestic OS / Linux** | UOS / Kylin / Deepin / Debian / Ubuntu | [⬇️ **ReadMD-linux-x86_64.deb**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.deb) | x86_64 Deb package; compatibility depends on native evidence for each system |
| **SHA-256 Hashes** | Checksum List | [⬇️ **SHA256SUMS.txt**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/SHA256SUMS.txt) | Complete SHA-256 integrity verification list |

Extract the complete Windows ZIP and keep its directory structure. Desktop startup requires Windows WebView2, macOS system WebKit, or Linux GTK / WebKitGTK. OCR, PDFium, Typst and publishing fonts are optional capability packs, separate from the base installer or ZIP; engine packs are currently prepared and tested only for Windows x64, with other platforms subject to the actual pack catalog. Linux ARM64 / Kylin ARM64 currently have build scripts only, with no V0.0.5 installer published.

---

## Multi-System & Native OS Integration

### 1. Linux & Chinese Domestic OS (KylinOS / UOS / Deepin)
- **Direct Installation**: Install [`ReadMD-linux-x86_64.deb`](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.deb), or extract [`ReadMD-linux-x86_64.tar.gz`](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.tar.gz) and run `ReadMD`. ARM64 build scripts are in [`scripts/linux`](scripts/linux); they do not represent published installers.
- **Environment Detection**: 100% pure native Rust kernel (`native_system`) detects OS distributions and dynamically adapts Wayland / X11 display backends.
- **Desktop Themes**: Probes DDE, UKUI, GNOME, and KDE dark mode settings via `gsettings`.
- **Desktop Entry**: Includes FreeDesktop launcher and MIME XML declaration.
- **Support boundary**: openEuler, Linglong and other unevidenced distributions are not claimed as fully supported in this release.

### 2. Windows & macOS
- **Windows**: Uses WebView2 windows and the system tray; the OCR engine is installed as an optional capability pack.
- **macOS**: Uses system WebKit windows; app builds and native feature validation are recorded separately.

---

<div align="center">

**ReadMD** · Pure local-first, distraction-free Markdown on tested supported platforms.

</div>
