<p align="center">
 <b>Languages / 多語言版本</b>:
 <a href="README.md">简体中文</a> |
 <b>繁體中文</b> |
 <a href="README.en.md">English</a> |
 <a href="README.ja.md">日本語</a>
</p>

<div align="center">
 <img src="assets/icon-256.png" width="88" alt="ReadMD logo">

 # ReadMD (Pure Rust Edition)

 **以 Rust 原生核心打造的跨平台 Markdown 閱讀器與編輯器**

 長文件語意分頁 · 顯示層非破壞修正 · Office/PDF/網頁轉 MD · 離线 OCR · LaTeX 學術增強 · MCP 整合 · 零 Python 相依

 官網地址：[https://rust.readmd.asia](https://rust.readmd.asia) ｜ 贊助支持（愛發電）：[https://ifdian.net/a/natsummerance](https://ifdian.net/a/natsummerance)

> [!IMPORTANT]
> **Rust 原生架構版（`rust-ReadMD`）**：閱讀、編輯、儲存與格式轉換由 Rust 核心管理，執行軟體不需要 Python 或 Office。桌面介面使用系統 WebView；大型排版、OCR 等能力透過獨立、可離線匯入的受管理能力套件提供。Windows 是目前主要實測平台；其他平台的建置產物與完整原生驗收分別記錄，請參閱 `release/platform-matrix.json`。

 [![platform](https://img.shields.io/badge/Windows%20%7C%20macOS%20%7C%20Linux%20%7C%20KylinOS%20%7C%20UOS-blue)](#全平台直接下載矩陣-release-assets)
 [![i18n](https://img.shields.io/badge/languages-46-orange)](docs/i18n-language-reference.md)
 [![license](https://img.shields.io/badge/license-MIT-green)](LICENSE)
 [![release](https://img.shields.io/github/v/release/Natsummerance/rust-ReadMD)](https://github.com/Natsummerance/rust-ReadMD/releases/latest)
 [![website](https://img.shields.io/badge/site-rust.readmd.asia-black)](https://rust.readmd.asia)
</div>

## 30 秒判斷適不適合你

| 你的情況 | ReadMD 的做法 |
| --- | --- |
| 文件超過約 8,000 行或 500KB | 啟用語義分頁，目錄與 Ctrl+F 搜尋跨頁可用 |
| 表格缺少分隔線或公式未閉合 | 僅在顯示層修復預覽，原始檔不會被改寫 |
| 手上有 DOCX / PPTX / XLSX / PDF / HTML | 轉換為 Markdown，減少重新排版時間 |
| 需要擷取圖片文字 | 在有對應 OCR 能力套件的平台離線辨識，套件依需求安裝 |
| 撰寫論文或技術文件 | 提供 BibTeX 引用卡片、定理與證明區塊 |
| 想要桌面伴讀或角色互動 | CC0 Arch-Chan Live2D 與 73 款微呼吸精靈圖，全端台詞同步 |
| 需要編輯 PDF | 在支援的平台安裝 PDFium 功能套件，編輯後儲存為 PDF；支援範圍見功能狀態 |
| 想接入 AI 工作流 | 提供 VS Code 擴充套件與 Rust stdio MCP Server |
| 希望使用自己的介面語言 | 提供 46 種介面語言，持續檢查詞條、預留位置及長文版面 |

## 直接下載

[Windows 安裝版](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMDSetup-windows-x64.exe) ·
[Windows 免安裝版](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-windows-x64.zip) ·
[macOS Apple Silicon](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-arm64.zip) ·
[macOS Intel](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-x64.zip) ·
[Linux tar.gz](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.tar.gz) ·
[UOS / 麒麟 Deb](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.deb) ·
[VS Code 擴充套件](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.5/readmd-vscode-0.0.6.vsix) ·
[MCP 設定套件](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.5/readmd-mcp-server-0.0.6.zip) ·
[SHA-256](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/SHA256SUMS.txt)

## 三步開始

1. 下載安裝版或免安裝版。
2. 開啟一個真實的大型文件，檢查目錄、搜尋和表格渲染。
3. 需要修改時再儲存；顯示修復不會未經同意寫入原始檔。

## 桌面伴讀寵物系統 (Live2D & 73 款微呼吸精靈圖)

- **CC0 Arch-Chan Live2D**：高精度 Live2D 骨骼動畫，隨閱讀進度與時段動態互動，支援平滑拖曳、滑鼠穿透與靈敏點擊。
- **73 款 Petdex 精靈圖自適應與活態微呼吸**：內建幾何自動推斷演算法，無延遲智慧適配高密序列幀與單立繪小圖，腳底錨點正弦微呼吸浮動賦予生動生命感。
- **伴侶生活狀態引擎**：等級、體力、心情、親密度成長系統，支援摸摸頭、餵食、玩耍、休息、喚醒等互動。
- **全端氣泡連動**：桌面懸浮窗、應用內小部件、設定面板舞台三端即時同步對話台詞，Electron 右鍵選單無縫下發。
- **高定蘋果風原生滑塊 (/apple-design)**：採用 Apple HIG 4px 極簡精緻軌道與 18px 實體物理質感懸浮圓鈕，Active 觸覺光暈與等寬數字藥丸讀數。

---

## 原生高保真向量 PDF 編輯器

- **結構與微觀審計**：自動審計 PDF 幾何尺寸、DPI、旋轉、系統檔案鎖，並對指定區域進行底色與噪點取樣。
- **零污染差分質檢 (Zero-Contamination Gate)**：沙箱模擬編輯並自動生成對比圖與熱力圖，未通過複核絕不寫入原始檔案。
- **安全無損落盤與一鍵復原**：落盤前強制生成 `.bak` 實體備份，解鎖 Windows 唯讀屬性；支援一鍵復原原始狀態。

---

## 為什麼值得 Star

ReadMD 處理長期資料庫裡的實際問題：大型文件能繼續閱讀，匯入資料減少手工整理，敏感草稿留在本機，原始檔仍保有最終決定權。如果你在 Windows、macOS、Linux 或國產系統之間切換，它能提供一致的工作流。

如果它幫你省下一次整理時間，請[給倉庫 Star](https://github.com/Natsummerance/rust-ReadMD)，讓更多創作者找到這個工具。

## 全平台直接下載矩陣 (Release Assets)

提供下載檔案不代表所有平台均已完成原生驗收；實際支援範圍與證據請見 `release/platform-matrix.json`。Windows 7/8、HarmonyOS/OpenHarmony 與未發行架構不在 V0.0.5 發行範圍。

| 作業系統 / 平台 | 架構 / 格式 | 直接下載連結 (GitHub Release) | 說明 |
| :--- | :--- | :--- | :--- |
| **Windows** | x64 (安裝版) | [⬇️ **ReadMDSetup-windows-x64.exe**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMDSetup-windows-x64.exe) | Windows x64 安裝套件；預設開啟方式由系統設定選擇 |
| **Windows** | x64 (免安裝便攜版) | [⬇️ **ReadMD-windows-x64.zip**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-windows-x64.zip) | 完整解壓縮 ZIP，保留 assets 與執行階段程式庫，再執行 ReadMD.exe |
| **macOS** | Apple Silicon (M系列) | [⬇️ **ReadMD-macos-arm64.zip**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-arm64.zip) | Apple Silicon Mac 應用程式套件；原生驗收狀態請見平台矩陣 |
| **macOS** | Intel x86_64 | [⬇️ **ReadMD-macos-x64.zip**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-macos-x64.zip) | Intel Mac 應用程式套件；原生驗收狀態請見平台矩陣 |
| **Linux** | x86_64 (tar.gz) | [⬇️ **ReadMD-linux-x86_64.tar.gz**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.tar.gz) | 解壓縮後執行 ReadMD；需要系統 GTK / WebKitGTK |
| **國產信創系統** | 統信 UOS / 銀河麒麟 / Deepin / Ubuntu | [⬇️ **ReadMD-linux-x86_64.deb**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/ReadMD-linux-x86_64.deb) | x86_64 Deb 套件；相容性以各系統的原生驗收記錄為準 |
| **SHA-256 驗證** | 雜湊清單 | [⬇️ **SHA256SUMS.txt**](https://github.com/Natsummerance/rust-ReadMD/releases/latest/download/SHA256SUMS.txt) | 發行檔案 SHA-256 完整性雜湊清單 |

Windows ZIP 必須完整解壓縮並保留目錄結構。桌面啟動需要 Windows WebView2、macOS 系統 WebKit 或 Linux GTK / WebKitGTK。OCR、PDFium、Typst 與出版字型為選用能力套件，不包含在基本安裝套件或 ZIP 中；目前引擎套件僅 Windows x64 已準備並實測，其他平台以實際套件目錄為準。Linux ARM64 / 麒麟 ARM64 目前僅有建置指令碼，未發行 V0.0.5 安裝套件。

---

<div align="center">

**ReadMD** · 純本機優先，在已驗證的支援平台閱讀寫作。

</div>
