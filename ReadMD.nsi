; ReadMD Windows NSIS Installer Script
; Generates dist\ReadMDSetup-windows-x64.exe

!include "MUI2.nsh"
!include "FileFunc.nsh"
!include "LogicLib.nsh"
!include "installer\capability-components.nsh"
!ifndef APP_VERSION
  !define APP_VERSION "0.0.5"
!endif

Name "ReadMD"
OutFile "dist\ReadMDSetup-windows-x64.exe"
InstallDir "$LOCALAPPDATA\ReadMD"
InstallDirRegKey HKCU "Software\ReadMD" "Install_Dir"
RequestExecutionLevel user

!define MUI_ABORTWARNING
!define MUI_ICON "assets\readmd.ico"
!define MUI_UNICON "assets\readmd.ico"

; Pages
!insertmacro MUI_PAGE_WELCOME
!insertmacro MUI_PAGE_DIRECTORY
!insertmacro MUI_PAGE_COMPONENTS
!insertmacro MUI_PAGE_INSTFILES
!insertmacro MUI_PAGE_FINISH

!insertmacro MUI_UNPAGE_CONFIRM
!insertmacro MUI_UNPAGE_INSTFILES

; Languages
!insertmacro MUI_LANGUAGE "SimpChinese"
!insertmacro MUI_LANGUAGE "English"

LangString CoreName ${LANG_SIMPCHINESE} "ReadMD（必需；已含 Markdown、Word、电子书、基础 PDF 与图表能力）"
LangString CoreName ${LANG_ENGLISH} "ReadMD (required; Markdown, Word, EPUB, basic PDF and charts included)"
LangString PdfName ${LANG_SIMPCHINESE} "PDF 原位编辑（下载 ${PDF_PACK_DOWNLOAD_MIB} MiB；安装后 ${PDF_PACK_INSTALLED_MIB} MiB）"
LangString PdfName ${LANG_ENGLISH} "PDF object editing (${PDF_PACK_DOWNLOAD_MIB} MiB download; ${PDF_PACK_INSTALLED_MIB} MiB installed)"
LangString TypstName ${LANG_SIMPCHINESE} "学术 PDF 排版（下载 ${TYPST_PACK_DOWNLOAD_MIB} MiB；安装后 ${TYPST_PACK_INSTALLED_MIB} MiB）"
LangString TypstName ${LANG_ENGLISH} "Academic PDF typesetting (${TYPST_PACK_DOWNLOAD_MIB} MiB download; ${TYPST_PACK_INSTALLED_MIB} MiB installed)"
LangString OcrName ${LANG_SIMPCHINESE} "离线中英文识别（下载 ${OCR_PACK_DOWNLOAD_MIB} MiB；安装后 ${OCR_PACK_INSTALLED_MIB} MiB）"
LangString OcrName ${LANG_ENGLISH} "Offline Chinese/English OCR (${OCR_PACK_DOWNLOAD_MIB} MiB download; ${OCR_PACK_INSTALLED_MIB} MiB installed)"
LangString FontsName ${LANG_SIMPCHINESE} "中文出版字体（下载 ${FONTS_PACK_DOWNLOAD_MIB} MiB；安装后 ${FONTS_PACK_INSTALLED_MIB} MiB）"
LangString FontsName ${LANG_ENGLISH} "Chinese publication fonts (${FONTS_PACK_DOWNLOAD_MIB} MiB download; ${FONTS_PACK_INSTALLED_MIB} MiB installed)"
LangString FontsDescription ${LANG_SIMPCHINESE} "选装。PDF 使用的黑体、宋体风格常规及粗体，不注册到系统。数学字体已随主程序提供；不依赖 Office 或系统中文字体。"
LangString FontsDescription ${LANG_ENGLISH} "Optional. Private regular/bold sans and serif fonts for PDF. Math font is already included in the core. No Office or system Chinese font installation."
LangString OcrDescription ${LANG_SIMPCHINESE} "推荐，可取消选择。自带简中、繁中、英文模型，无需 Office、Python 或系统识别语言包。扫描 PDF 另外使用 PDF 原位编辑引擎。"
LangString OcrDescription ${LANG_ENGLISH} "Recommended; may be deselected. Includes Simplified/Traditional Chinese and English models. No Office, Python or OS OCR languages required. Scanned PDFs also use the PDF object engine."
LangString PdfDescription ${LANG_SIMPCHINESE} "选装。编辑 PDF 文字、图片、批注与页面。不需要 Acrobat。安装时联网下载经校验的 ReadMD 能力包；以后也能在功能入口安装或离线导入。"
LangString PdfDescription ${LANG_ENGLISH} "Optional. Edit PDF text, images, annotations and pages without Acrobat. Downloads a verified ReadMD capability pack; also available later at the feature entry or by offline import."
LangString TypstDescription ${LANG_SIMPCHINESE} "选装。真实分页、论文双栏、矢量公式与科研图表。不需要 Office 或预装 Typst。安装时下载锁定的独立运行包。"
LangString TypstDescription ${LANG_ENGLISH} "Optional. True pagination, two-column papers, vector math and research figures. No Office or system Typst required. Downloads a pinned standalone runtime."
LangString PackFailure ${LANG_SIMPCHINESE} "能力包安装未完成，基础软件仍可使用。可稍后在功能入口重试，或导入离线能力包。"
LangString PackFailure ${LANG_ENGLISH} "The optional pack was not installed. ReadMD remains usable. Retry at the feature entry or import an offline capability pack later."

Section "$(CoreName)" SEC01
  SectionIn RO
  SetOutPath "$INSTDIR"
  SetOverwrite on
  
  File "dist\ReadMD-windows-x64\ReadMD.exe"
  File "dist\ReadMD-windows-x64\vcruntime140.dll"
  File "dist\ReadMD-windows-x64\vcruntime140_1.dll"
  File "dist\ReadMD-windows-x64\ReadMD-Pet-Rust.zip"
  File /r "dist\ReadMD-windows-x64\assets"
  File "dist\ReadMD-windows-x64\LICENSE"
  File "dist\ReadMD-windows-x64\README.md"
  
  ; Write install dir to registry
  WriteRegStr HKCU "Software\ReadMD" "Install_Dir" "$INSTDIR"
  
  ; Shortcuts
  CreateDirectory "$SMPROGRAMS\ReadMD"
  CreateShortcut "$SMPROGRAMS\ReadMD\ReadMD.lnk" "$INSTDIR\ReadMD.exe" "" "$INSTDIR\ReadMD.exe" 0
  CreateShortcut "$SMPROGRAMS\ReadMD\Uninstall ReadMD.lnk" "$INSTDIR\Uninstall.exe" "" "$INSTDIR\Uninstall.exe" 0
  CreateShortcut "$DESKTOP\ReadMD.lnk" "$INSTDIR\ReadMD.exe" "" "$INSTDIR\ReadMD.exe" 0
  
  ; File Associations for .md and .markdown
  WriteRegStr HKCU "Software\Classes\.md" "" "ReadMD.Markdown"
  WriteRegStr HKCU "Software\Classes\.markdown" "" "ReadMD.Markdown"
  WriteRegStr HKCU "Software\Classes\ReadMD.Markdown" "" "Markdown Document"
  WriteRegStr HKCU "Software\Classes\ReadMD.Markdown\DefaultIcon" "" '"$INSTDIR\assets\markdown-file.ico",0'
  WriteRegStr HKCU "Software\Classes\ReadMD.Markdown\shell\open\command" "" '"$INSTDIR\ReadMD.exe" "%1"'
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
  
  ; Uninstaller & Control Panel Add/Remove entry
  WriteUninstaller "$INSTDIR\Uninstall.exe"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ReadMD" "DisplayName" "ReadMD - Markdown Editor & Reader"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ReadMD" "UninstallString" '"$INSTDIR\Uninstall.exe"'
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ReadMD" "DisplayIcon" "$INSTDIR\ReadMD.exe,0"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ReadMD" "DisplayVersion" "${APP_VERSION}"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ReadMD" "Publisher" "Natsummerance"
  WriteRegStr HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ReadMD" "URLInfoAbout" "https://rust.readmd.asia"
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ReadMD" "NoModify" 1
  WriteRegDWORD HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ReadMD" "NoRepair" 1
SectionEnd

Section "$(OcrName)" SEC_OCR
  AddSize ${OCR_PACK_INSTALLED_KIB}
  nsExec::ExecToStack '"$INSTDIR\ReadMD.exe" --install-capability ocr --assets "$INSTDIR\assets"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "$(PackFailure)"
    MessageBox MB_OK|MB_ICONEXCLAMATION "$(PackFailure)" /SD IDOK
  ${EndIf}
SectionEnd
Section /o "$(PdfName)" SEC_PDF
  AddSize ${PDF_PACK_INSTALLED_KIB}
  nsExec::ExecToStack '"$INSTDIR\ReadMD.exe" --install-capability pdfium --assets "$INSTDIR\assets"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "$(PackFailure)"
    MessageBox MB_OK|MB_ICONEXCLAMATION "$(PackFailure)" /SD IDOK
  ${EndIf}
SectionEnd
Section /o "$(TypstName)" SEC_TYPST
  AddSize ${TYPST_PACK_INSTALLED_KIB}
  nsExec::ExecToStack '"$INSTDIR\ReadMD.exe" --install-capability typst --assets "$INSTDIR\assets"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "$(PackFailure)"
    MessageBox MB_OK|MB_ICONEXCLAMATION "$(PackFailure)" /SD IDOK
  ${EndIf}
SectionEnd
Section /o "$(FontsName)" SEC_FONTS
  AddSize ${FONTS_PACK_INSTALLED_KIB}
  nsExec::ExecToStack '"$INSTDIR\ReadMD.exe" --install-capability fonts --assets "$INSTDIR\assets"'
  Pop $0
  Pop $1
  ${If} $0 != 0
    DetailPrint "$(PackFailure)"
    MessageBox MB_OK|MB_ICONEXCLAMATION "$(PackFailure)" /SD IDOK
  ${EndIf}
SectionEnd
!insertmacro MUI_FUNCTION_DESCRIPTION_BEGIN
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC01} "$(CoreName)"
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_OCR} "$(OcrDescription)"
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_PDF} "$(PdfDescription)"
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_TYPST} "$(TypstDescription)"
  !insertmacro MUI_DESCRIPTION_TEXT ${SEC_FONTS} "$(FontsDescription)"
!insertmacro MUI_FUNCTION_DESCRIPTION_END

Section "Uninstall"
  Delete "$DESKTOP\ReadMD.lnk"
  Delete "$SMPROGRAMS\ReadMD\ReadMD.lnk"
  Delete "$SMPROGRAMS\ReadMD\Uninstall ReadMD.lnk"
  RMDir "$SMPROGRAMS\ReadMD"
  
  RMDir /r "$INSTDIR\assets"
  Delete "$INSTDIR\ReadMD.exe"
  Delete "$INSTDIR\vcruntime140.dll"
  Delete "$INSTDIR\vcruntime140_1.dll"
  Delete "$INSTDIR\ReadMD-Pet-Rust.zip"
  Delete "$INSTDIR\LICENSE"
  Delete "$INSTDIR\README.md"
  Delete "$INSTDIR\Uninstall.exe"
  RMDir "$INSTDIR"
  
  DeleteRegKey HKCU "Software\Classes\ReadMD.Markdown"
  DeleteRegKey HKCU "Software\Microsoft\Windows\CurrentVersion\Uninstall\ReadMD"
  DeleteRegKey HKCU "Software\ReadMD"
SectionEnd
