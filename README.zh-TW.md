<p align="center">
 <b>Languages</b>:
 <a href="README.md">简体中文</a> |
 <b>繁體中文</b> |
 <a href="README.en.md">English</a> |
 <a href="README.ja.md">日本語</a>
</p>

<div align="center">
 <img src="assets/icon-256.png" width="88" alt="ReadMD logo">

 # ReadMD (Pure Rust Edition)

 **以 Rust 原生核心打造的跨平台 Markdown 閱讀器與編輯器**

 長文件語意分頁 · 非破壞顯示修復 · Office/PDF 原版閱讀與擷取 · 表格編輯與出版 · AI 審閱 · VS Code / MCP

 [官網](https://rust.readmd.asia) ｜ [V0.0.7 發行說明](release/release_notes.md) ｜ [贊助支持](https://ifdian.net/a/natsummerance)

 [![platform](https://img.shields.io/badge/Windows%2010%2F11%20%7C%20macOS%2013%2B%20%7C%20Linux-blue)](#下載與安裝)
 [![i18n](https://img.shields.io/badge/languages-46-orange)](docs/i18n-language-reference.md)
 [![license](https://img.shields.io/badge/license-MIT-green)](LICENSE)
 [![release](https://img.shields.io/github/v/release/Natsummerance/rust-ReadMD)](https://github.com/Natsummerance/rust-ReadMD/releases/latest)
</div>

ReadMD 將閱讀、編輯、格式轉換與出版放在同一個本機工作流程中。Rust 核心管理正文與儲存，桌面介面使用系統 WebView，執行應用程式無需安裝 Python 或 Microsoft Office。較大的排版、OCR、字型與媒體功能依平台透過受管理的功能套件提供，支援離線匯入。

## 30 秒判斷適不適合你

| 你的情況 | ReadMD 的做法 |
| --- | --- |
| 文件超過約 8,000 行或 500 KB | 自動啟用語意分頁，目錄與全文搜尋跨頁連動 |
| 表格缺分隔線或公式未閉合 | 在記憶體中修復顯示；明確編輯並儲存才改寫原始檔 |
| 需要先看 PDF、DOCX 或 PPTX | 先閱讀頁面，再依需求擷取 Markdown；Office 原排版檢視為唯讀 |
| 需要整理複雜表格 | 合併／分割儲存格、調整列高與欄寬、複製貼上合併區域，並復原修改 |
| 撰寫論文、書籍或技術資料 | 使用引用、公式、定理區塊和出版預設，匯出 PDF、DOCX、EPUB、HTML、TeX、PPTX |
| 希望 AI 協助修改 | 明確選擇連線與模型，逐區塊審閱修改，保留草稿與復原記錄 |
| 在編輯器或 AI 用戶端中工作 | 使用配套 VS Code 擴充套件和 Rust stdio MCP 入口 |

## V0.0.7 更新

- **先閱讀，再擷取**：PDF 預設進入原版頁面閱讀；DOCX／PPTX 新增唯讀原排版閱讀。擷取時回報格式損失，保護原始檔與既有同名 Markdown。
- **表格與出版**：新增矩形合併、分割、儲存格屬性、列高欄寬及合併區域貼上；PDF／DOCX／PPTX 輸出保留各自支援的表格版面。
- **儲存與復原**：改善連續關閉分頁、編碼確認、圖片與歷史跨目錄另存、待審修改撤回、批次轉換回執及媒體工作續作。
- **共享審閱與繪圖**：衝突建議可對照原文、目前內容與建議手動合併；繪圖儲存為 SVG，支援重開、復原及草稿恢復。
- **開發者與 AI**：多檔 OpenAPI 本機引用與更新共用產生服務；來源變更會讓舊預覽失效。聊天取消、失敗及隱私模式採用一致的歷史儲存規則。 發行收尾亦涵蓋 SSE 接收停滯時的取消，以及 Office 固定相依套件授權通知的完整性。

詳見[本版說明](release/release_notes.md)；[歷次更新](RELEASE_NOTES.md)保留舊版記錄。

本次同版更新加入五套原生 PPTX 設計：**合流、萬象光場、浮光、液相、在場**，正文與表格以可編輯物件寫入，保留各套設計的原生背景與可用動畫。HTML 提供**出版閱讀、技術文件、長文雜誌**三種連續閱讀版式，預覽與獨立檔案共用正文、目錄、排印及寬表格處理。見[範本說明](rust/readmd-kernel/templates/pptx/README.md)。

此前排版发布的全部 10 份桌面／整合資產由同一原始碼提交 [`43905601f2a0ae747f4000aad8e69a80e955ce7c`](https://github.com/Natsummerance/rust-ReadMD/tree/43905601f2a0ae747f4000aad8e69a80e955ce7c) 建置；[對應原始碼 ZIP](https://github.com/Natsummerance/rust-ReadMD/archive/43905601f2a0ae747f4000aad8e69a80e955ce7c.zip) · [本次發行 CI](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44669807015)。原 `V0.0.7` 標籤保留首次發行時的提交，本輪原始碼請使用這裡的固定連結。

<!-- readmd-autopublish:start -->
圖表閱讀介面與 AI 修復原始碼已公開：[`f6267fdf820135bcb2b2f2dd56313e0ee2160646`](https://github.com/Natsummerance/rust-ReadMD/tree/f6267fdf820135bcb2b2f2dd56313e0ee2160646)。[發行 CI](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44750315786) 與 [PET 三系統檢查](https://github.com/Natsummerance/rust-ReadMD/actions/runs/44750341039) 均通過後，GitHub 自動續作統一替換全部十份桌面／整合套件和校驗清單，再同步實際下載快照與官網；目前正式檔案以 Release 資產與 SHA256SUMS 為準。
<!-- readmd-autopublish:end -->

本輪原始碼讓閱讀中的圖表直接融入正文，原始碼與複製控制項預設摺疊。繪製失敗時提供錯誤定位和本機修復預覽；使用者可主動把目前圖表及診斷傳給已設定的 AI，產生可編輯候選，先預覽再提出待審修改，最後明確儲存。

## 下載與安裝

桌面套件、擴充套件及 MCP 設定套件統一為 **V0.0.7**。[全部檔案與 SHA-256](https://github.com/Natsummerance/rust-ReadMD/releases/tag/V0.0.7)。

| 平台 | 下載 | 安裝說明 |
| --- | --- | --- |
| Windows 10/11 x64 | [安裝版](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMDSetup-windows-x64.exe) · [可攜 ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-windows-x64.zip) | 完整解壓縮並保留 assets 與執行階段程式庫，再執行 `ReadMD.exe`；需要 WebView2 |
| macOS 13+ · Apple Silicon | [DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-arm64.zip) | 將 `ReadMD.app` 移至「應用程式」；見 [Mac 安裝說明](release/macos-install.md) |
| macOS 13+ · Intel | [DMG](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.dmg) · [ZIP](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-macos-x64.zip) | 選擇 x64 套件，使用系統 WebKit |
| Linux x86_64 | [DEB](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.deb) · [tar.gz](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/ReadMD-linux-x86_64.tar.gz) | 需要 GTK3、WebKitGTK 4.1 等系統程式庫；DEB 列出所需相依套件 |
| VS Code／相容編輯器 | [readmd-vscode-0.0.7.vsix](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-vscode-0.0.7.vsix) | 安裝桌面端後，選擇「從 VSIX 安裝」；見[擴充套件說明](packages/vscode-extension/README.md) |
| MCP 用戶端 | [readmd-mcp-server-0.0.7.zip](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-mcp-server-0.0.7.zip) | 文件與連線範本；核心使用已安裝桌面程式的 `--mcp` 入口 |

下載後可用 [SHA256SUMS.txt](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/SHA256SUMS.txt) 核對檔案。替換舊版前先結束應用程式，個人文件無需移動。

Mac 套件採用**免費 ad hoc 簽署，未使用 Developer ID，未經 Apple 公證**。首次開啟可能遭 Gatekeeper 阻擋；依[安裝說明](release/macos-install.md)在「系統設定 → 隱私權與安全性」中允許 ReadMD 開啟。

本版發行目標為 Windows x64、Linux x64、macOS ARM64／Intel，另提供 VSIX 及 MCP 設定套件。UOS、銀河麒麟、Deepin 等系統的相容性取決於系統程式庫與對應實機記錄。Windows／Linux ARM64、AppImage、HarmonyOS／OpenHarmony 不在本版下載檔案內。

## 原版閱讀與選用功能

桌面套件包含對應平台的 Office 閱讀執行階段與離線 PDFium 套件。應用程式依需求準備資源，功能狀態會區分安裝、探測和實際操作。Office 原排版檢視使用隨附排版引擎，複雜文件的顯示可能與 Word／PowerPoint 有差異；擷取 Markdown 時會明確回報無法保留的內容。

OCR、Typst、出版字型、程式碼執行階段與音訊／影片轉寫依實際平台目錄提供。掃描 PDF 需要 OCR 與 PDFium；轉寫需要對應平台的 FFmpeg、Whisper 引擎及模型。**FFmpeg／Whisper 獨立離線套件未包含在上表的桌面與整合下載檔案中**。本輪媒體套件實測範圍為 macOS ARM64，其他平台依實際可取得的套件與功能狀態為準。

PDF 預設以閱讀模式開啟；選擇「編輯原 PDF」後可進行支援的編輯操作，儲存前保留復原與來源衝突保護。PDF 表單、簽章、權限及複雜 Office 往返依各格式支援範圍處理。PPTX 公式以圖片保留，原始碼存於備註。

## 本機工作流程與 AI

- 閱讀、編輯與本機轉換在電腦上執行；網頁擷取、AI、郵件等連網操作需主動呼叫並設定服務。
- AI 結果可逐區塊審閱；失敗、取消和隱私聊天不寫入未完成的聊天歷史。儲存正文、接受修改與工作完成分別顯示狀態。
- 分享支援區域網路閱讀、編輯與建議審閱；共享內容在本機明確儲存後才寫入文件。
- 可使用 BibTeX 卡片、學術區塊、公式編輯、離線 HTML 簡報與桌面伴讀寵物。桌寵全域鍵鼠監聽依 Windows 支援範圍提供。
- 提供 46 種介面語言；鍵與預留位置有自動檢查，部分新增工作流程提示仍使用英文。

## VS Code 與 MCP

安裝 [VSIX](https://github.com/Natsummerance/rust-ReadMD/releases/download/V0.0.7/readmd-vscode-0.0.7.vsix) 後，使用命令選擇區中的 ReadMD 命令進行預覽、修復、轉換、匯出、工作區搜尋與 OpenAPI 文件更新。需要時將 `readmd.executablePath` 設為桌面執行檔的絕對路徑。行內 AI 補全預設關閉，可在信任的工作區啟用。詳見[擴充套件文件](packages/vscode-extension/README.md)。

MCP 與桌面共用 Rust 核心，不需要 Python。將以下 `command` 換成實際執行檔的絕對路徑：

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

macOS 通常為 `/Applications/ReadMD.app/Contents/MacOS/ReadMD`；Windows 使用 `ReadMD.exe` 的完整路徑，JSON 中反斜線寫作 `\\`。更多範本見 [MCP 說明](packages/mcp-server/README.md)。

## 驗證與原始碼

此前排版发布的正式 Apple Silicon ZIP 已在本機完成兩次原生冷啟動／重開，並更新安裝。Windows／Linux 由 CI 中實際發行執行檔的生命週期與圖像匯出回歸涵蓋；Intel 由 Mac CI 建置、簽章及封裝檢查涵蓋。

2026-11-19 首次發行的 Mac 本機候選版在 ARM64 上完成六組共 **244 項原生檢查**，涵蓋關閉與復原、六格式匯出、候選版 ZIP 冷啟動／重開及 V0.0.6 隔離工作階段升級；另使用封存套件中的正式執行檔完成 46 語言的儲存編碼流程，頁面例外為 0。[Mac 驗證記錄](docs/reviews/readmd-v007-macos-rc-2026-11-19.md)保留一次 WebKit 冷啟動首敗與同包複測結果。這些檢查範圍與跨應用相容性、各系統實機覆蓋分別記錄。

2026-11-19 首次發行時，Windows x64、Linux x64、macOS ARM64／Intel 的發行建置與共用品質檢查已通過。該次正式 ARM64 下載 ZIP 另完成正常系統冷啟動與重開，原測試文件位元組保持；本次正式套件與早期 244 項候選版檢查分別記錄。見[發行驗證記錄](docs/reviews/readmd-v007-published-2026-11-19.md)與[平台矩陣](release/platform-matrix.json)。

```sh
cargo build --locked --release --manifest-path rust/Cargo.toml -p readmd-kernel
```

桌面執行檔位於 `rust/target/release/`。分發還需要前端資源與平台執行階段，相關指令碼位於 [tools](tools) 及 [scripts](scripts)。選用程式碼執行需要對應語言執行階段。

[產品索引](https://rust.readmd.asia/llms.txt) · [完整產品資料](https://rust.readmd.asia/llms-full.txt) · [問題回報](https://github.com/Natsummerance/rust-ReadMD/issues) · [MIT 授權](LICENSE)
