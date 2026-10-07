import * as vscode from 'vscode';
import * as path from 'path';
import * as fs from 'fs';
import { ReadMDBridge } from './bridge';
import { ReadMDToolboxProvider } from './sidebarProvider';
import { l10n } from './localization';
import { getWebviewContent, WebviewAssets } from './webview';
import { fencedCodeAt } from './fences';
import { registerIntelligence } from './intelligence';

let diagnosticStatusBarItem: vscode.StatusBarItem;
let coreStatusBarItem: vscode.StatusBarItem;
let coreWasDown = false;

/** Keep Core error codes out of the UI and out of user documents. */

function errorText(error: unknown): string {
  const code = error instanceof Error ? error.message : String(error || '');
  const messages: Record<string, [string, string]> = {
    core_process_exit: ['errCoreProcessExit', 'ReadMD Core stopped; try again'],
    core_start_timeout: ['errCoreStartTimeout', 'ReadMD Core startup timed out'],
    core_not_connected: ['errCoreNotConnected', 'ReadMD Core is not connected'],
    core_operation_timeout: ['errCoreOperationTimeout', 'The operation timed out; try again'],
    core_closed: ['errCoreClosed', 'ReadMD Core is closed'],
    readmd_binary_not_found: ['errBinaryNotFound', 'ReadMD is not installed or not on PATH; install the desktop app or set readmd.executablePath'],
    readmd_binary_invalid: ['errBinaryInvalid', 'readmd.executablePath does not point to a working ReadMD executable'],
    confirmation_required: ['errConfirmationRequired', 'This action needs confirmation'],
    output_exists: ['errOutputExists', 'The target file already exists'],
    output_path_must_be_absolute: ['errOutputPathAbsolute', 'The output path must be absolute'],
    output_directory_not_found: ['errOutputDirMissing', 'The output folder does not exist'],
    file_not_found: ['errFileNotFound', 'File not found'],
    mcp_request_failed: ['errMcpRequestFailed', 'Core request failed'],
    mcp_tool_failed: ['errMcpToolFailed', 'Core tool failed'],
    ai_cancelled: ['errAiCancelled', 'AI generation cancelled'],
    core_configuration_changed: ['errCoreConfigChanged', 'Core settings changed; try the operation again.'],
    core_protocol_unsupported: ['errCoreProtocol', 'The installed ReadMD Core uses an unsupported protocol. Update the desktop app.'],
    core_response_too_large: ['errCoreResponseSize', 'The result exceeds the 32 MB limit.'],
    core_request_too_large: ['errCoreRequestSize', 'The request exceeds 32 MB. Select a smaller section or split the document.'],
    unknown_style_preset: ['errUnknownPreset', 'This export preset no longer exists. Choose another preset.'],
  };
  const pair = messages[code];
  if (pair) return l10n(pair[0], pair[1]);
  if (code && (code.startsWith('ReadMD:') || code.includes(' '))) return code;
  return l10n('errOperationFailed', 'Operation failed; try again');
}

export function parseJsoncSafely(text: string): Record<string, any> {
  let inString = false;
  let inLineComment = false;
  let inBlockComment = false;
  let stringEscape = false;
  const chars = text.split('');

  for (let i = 0; i < chars.length; i++) {
    const ch = chars[i];
    const next = chars[i + 1];

    if (inLineComment) {
      if (ch === '\n' || ch === '\r') {
        inLineComment = false;
      } else {
        chars[i] = ' ';
      }
      continue;
    }

    if (inBlockComment) {
      if (ch === '*' && next === '/') {
        chars[i] = ' ';
        chars[i + 1] = ' ';
        i++;
        inBlockComment = false;
      } else {
        if (ch !== '\n' && ch !== '\r') chars[i] = ' ';
      }
      continue;
    }

    if (inString) {
      if (stringEscape) {
        stringEscape = false;
      } else if (ch === '\\') {
        stringEscape = true;
      } else if (ch === '"') {
        inString = false;
      }
      continue;
    }

    if (ch === '"') {
      inString = true;
      continue;
    }

    if (ch === '/' && next === '/') {
      inLineComment = true;
      chars[i] = ' ';
      chars[i + 1] = ' ';
      i++;
      continue;
    }

    if (ch === '/' && next === '*') {
      inBlockComment = true;
      chars[i] = ' ';
      chars[i + 1] = ' ';
      i++;
      continue;
    }
  }

  if (inString || inBlockComment) throw new Error('mcp_invalid_config');
  const finalChars = chars;
  inString = false;
  stringEscape = false;
  for (let i = 0; i < finalChars.length; i++) {
    const ch = finalChars[i];
    if (inString) {
      if (stringEscape) {
        stringEscape = false;
      } else if (ch === '\\') {
        stringEscape = true;
      } else if (ch === '"') {
        inString = false;
      }
      continue;
    }
    if (ch === '"') {
      inString = true;
      continue;
    }
    if (ch === ',') {
      let j = i + 1;
      while (j < finalChars.length && (finalChars[j] === ' ' || finalChars[j] === '\t' || finalChars[j] === '\n' || finalChars[j] === '\r')) {
        j++;
      }
      if (j < finalChars.length && (finalChars[j] === '}' || finalChars[j] === ']')) {
        finalChars[i] = ' ';
      }
    }
  }

  const parsed = JSON.parse(finalChars.join('').replace(/^\uFEFF/, ''));
  if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
    return parsed;
  }
  throw new Error('mcp_invalid_config');
}

export function activate(context: vscode.ExtensionContext) {
  const bridge = new ReadMDBridge(context);
  context.subscriptions.push({ dispose: () => bridge.dispose() });
  registerIntelligence(context, bridge);
  const trusted = () => {
    if (vscode.workspace.isTrusted !== false) return true;
    void vscode.window.showWarningMessage(l10n('trustRequired', 'Trust this workspace before using native tools, AI or code execution.'));
    return false;
  };
  const customCssFile = (): string | undefined => {
    if (vscode.workspace.isTrusted === false) return;
    const configured = (vscode.workspace.getConfiguration?.('readmd').get<string>('customCssPath', '') || '').trim();
    if (!configured) return;
    const file = path.isAbsolute(configured) ? configured : path.resolve(vscode.workspace.workspaceFolders?.[0]?.uri.fsPath || context.extensionPath, configured);
    try { const stat = fs.statSync(file); if (stat.isFile() && stat.size <= 256 * 1024 && path.extname(file).toLowerCase() === '.css') return file; } catch {}
    void vscode.window.showWarningMessage(l10n('invalidCustomCss', 'The preview stylesheet must be an existing CSS file under 256 KB.'));
    return;
  };
  const webviewAssets = (webview: vscode.Webview, docDir?: string): WebviewAssets => ({
    base: webview.asWebviewUri(vscode.Uri.file(path.join(context.extensionPath, 'media'))).toString(),
    cspSource: webview.cspSource, language: vscode.env.language,
    documentBase: docDir ? webview.asWebviewUri(vscode.Uri.file(docDir)).toString().replace(/\/$/, '') + '/' : undefined,
    customStyle: (() => { const css = customCssFile(); return css ? webview.asWebviewUri(vscode.Uri.file(css)).toString() : undefined; })(),
  });
  const resourceRoots = (docDir: string) => {
    const roots = [vscode.Uri.file(path.join(context.extensionPath, 'media')), vscode.Uri.file(docDir)];
    const css = customCssFile(); if (css) roots.push(vscode.Uri.file(path.dirname(css)));
    return roots;
  };
  const openDocumentLink = async (href: string, docDir: string) => {
    if (/^[a-z][a-z\d+.-]*:|^\/\//i.test(href)) return;
    try {
      const file = path.resolve(docDir, decodeURIComponent(href.split(/[?#]/)[0]));
      if (!/\.(?:md|markdown|txt|tex|bib|json|ya?ml)$/i.test(file)) return;
      const document = await vscode.workspace.openTextDocument(vscode.Uri.file(file));
      await vscode.window.showTextDocument(document, vscode.ViewColumn.One);
    } catch { void vscode.window.showWarningMessage(l10n('errFileNotFound', 'File not found')); }
  };
  const previews = new Map<string, vscode.WebviewPanel>();
  const handleNativeDiagram = async (webview: vscode.Webview, message: any) => {
    if (message?.type !== 'nativeDiagram' || typeof message.token !== 'string') return;
    try {
      if (vscode.workspace.isTrusted === false) throw new Error('trust');
      const result = await bridge.callMcpTool('readmd_render_diagram', { engine: message.engine, code: message.code, confirm: true });
      await webview.postMessage({ type: 'nativeDiagramResult', token: message.token, svg: result.svg });
    } catch { await webview.postMessage({ type: 'nativeDiagramResult', token: message.token, error: true }); }
  };

  // Native Skill/AI entry points share the same persistent ReadMD Core
  // connection as conversion and preview commands.
  const skillsDisposable = vscode.commands.registerCommand('readmd.openSkills', async () => {
    if (!trusted()) return;
    try {
      const skills = await bridge.listSkills();
      const pick = await vscode.window.showQuickPick(skills.map((s: any) => ({
        label: s.name || s.uri, description: s.description || '', uri: s.uri,
      })), { placeHolder: l10n('pickSkill', 'Select ReadMD Skill') });
      if (!pick) return;
      const text = await bridge.readSkill(pick.uri);
      const doc = await vscode.workspace.openTextDocument({ content: text, language: 'markdown' });
      await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
    } catch (err: any) { vscode.window.showErrorMessage(l10n('skillsOpenFailed', `Failed to open ReadMD Skills: ${errorText(err)}`, { error: errorText(err) })); }
  });

  const aiWorkbenchDisposable = vscode.commands.registerCommand('readmd.openAiWorkbench', async () => {
    if (!trusted()) return;
    const editor = vscode.window.activeTextEditor;
    if (!editor) { vscode.window.showInformationMessage(l10n('openDocFirst', 'Please open a Markdown document first')); return; }
    const targetDoc = editor.document;
    const initialDocUri = targetDoc.uri.toString();
    const initialDocVersion = targetDoc.version;
    const initialSelection = editor.selection;
    const hasSelection = initialSelection.isEmpty === false;
    const sourceText = targetDoc.getText(hasSelection ? initialSelection : undefined);

    try {
      const prompts = await bridge.listPrompts();
      if (!prompts.length) {
        vscode.window.showWarningMessage(l10n('noSkillsAvailable', 'No Skills available in the current Core'));
        return;
      }
      const workflow = await vscode.window.showQuickPick(prompts.map((prompt: any) => ({
        label: prompt.name || prompt.skill_id,
        description: prompt.description || prompt.skill_id || '',
        id: prompt.name || prompt.skill_id,
        skillId: prompt.skill_id || prompt.name,
      })), { placeHolder: l10n('pickAiWorkflow', 'Select ReadMD AI Skill workflow') });
      if (!workflow) return;
      const providers = (await bridge.listProviders()).filter((p: any) => {
        let loopback = false;
        try { loopback = ['localhost', '127.0.0.1', '[::1]'].includes(new URL(p.base_url).hostname); } catch {}
        return p.credential_id || p.key_source || p.requires_key === false || p.local === true || p.category === 'local' || loopback;
      });
      if (!providers.length) {
        vscode.window.showWarningMessage(l10n('configureAiFirst', 'Please configure AI providers and credentials in the ReadMD desktop app first'));
        return;
      }
      const provider: any = await vscode.window.showQuickPick(providers.map((p: any) => ({
        label: p.name, description: p.has_key ? l10n('hasCredentials', 'Configured credentials') : l10n('usesEnvOrLocal', 'Using environment variables or local service'), value: p,
      })), { placeHolder: l10n('pickAiProvider', 'Select AI Provider') });
      if (!provider) return;
      let models: string[] = (provider.value.models || []).map((model: any) => typeof model === 'string' ? model : model?.id).filter(Boolean);
      if (!models.length) models = await bridge.listModels(provider.value.id, provider.value.credential_id);
      let modelPick: any = models.length > 1
        ? await vscode.window.showQuickPick([...models.map((m: string) => ({ label: m, value: m })), { label: l10n('refreshModels', 'Refresh models from connection…'), value: '', refresh: true }], { placeHolder: l10n('pickModel', 'Select Model') })
        : models[0] ? { value: models[0] } : undefined;
      if (modelPick?.refresh) {
        models = await bridge.listModels(provider.value.id, provider.value.credential_id);
        modelPick = await vscode.window.showQuickPick(models.map(model => ({ label: model, value: model })), { placeHolder: l10n('pickModel', 'Select Model') });
      }
      if (models.length && !modelPick) return;
      const model = modelPick?.value || '';
      if (!model) { vscode.window.showWarningMessage(l10n('noModelsAvailable', 'No models available for the selected provider, please refresh the model list')); return; }
      let output = '';
      let cancelled = false;
      await vscode.window.withProgress({
        location: vscode.ProgressLocation.Notification,
        title: l10n('aiGenerating', 'ReadMD AI is generating…'),
        cancellable: true,
      }, async (progress, token) => {
        const result: any = await bridge.aiChatStreaming({
          provider: provider.value.id, credential_id: provider.value.credential_id,
          model, skill_id: workflow.skillId, markdown_content: sourceText,
          language: vscode.env.language || 'en',
        }, chunk => {
          progress.report({ message: chunk.length > 48 ? `…${chunk.slice(-48)}` : chunk });
        }, token);
        if (result?.ok === false) {
          output = '';
          if (result.error_code !== 'ai_cancelled') {
            throw new Error(String(result.error_code || 'mcp_tool_failed'));
          }
          cancelled = true;
          return;
        }
        if (result?.content) output = String(result.content);
      });
      if (cancelled) { vscode.window.showInformationMessage(l10n('aiCancelled', 'ReadMD AI generation was cancelled')); return; }
      if (!output) { vscode.window.showWarningMessage(l10n('noAiOutput', 'AI did not return applicable content')); return; }
      const replaceLabel = hasSelection ? l10n('btnReplaceSelection', 'Replace Selection') : l10n('btnReplaceDocument', 'Replace Document');
      const choice = await vscode.window.showInformationMessage(l10n('aiResultGenerated', 'ReadMD AI result generated'), replaceLabel, l10n('btnInsertEnd', 'Insert at End'), l10n('btnViewOnly', 'View Only'));
      if (choice === replaceLabel) {
        const currentEditor = vscode.window.activeTextEditor;
        const isSameDoc = currentEditor && currentEditor.document.uri.toString() === initialDocUri;
        const isUnchanged = isSameDoc && currentEditor.document.version === initialDocVersion;

        if (!isUnchanged || !currentEditor) {
          const protectChoice = await vscode.window.showWarningMessage(
            l10n('docModifiedDuringAi', 'ReadMD: Document content was modified during AI generation; replacement aborted to prevent overwriting changes.'),
            l10n('btnViewBeside', 'View Result Beside'),
            l10n('btnInsertEnd', 'Insert at End')
          );
          if (protectChoice === l10n('btnViewBeside', 'View Result Beside') || protectChoice === '侧边查看') {
            const doc = await vscode.workspace.openTextDocument({ content: output, language: 'markdown' });
            await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
          } else if (protectChoice === l10n('btnInsertEnd', 'Insert at End') || protectChoice === '插入末尾') {
            if (currentEditor && currentEditor.document.uri.toString() === initialDocUri) {
              await currentEditor.edit(editBuilder =>
                editBuilder.insert(currentEditor.document.positionAt(currentEditor.document.getText().length), `\n\n${output}\n`)
              );
            }
          }
          return;
        }
        const range = hasSelection ? initialSelection : new vscode.Range(targetDoc.positionAt(0), targetDoc.positionAt(targetDoc.getText().length));
        await currentEditor.edit(editBuilder => editBuilder.replace(range, output));
      } else if (choice === l10n('btnInsertEnd', 'Insert at End') || choice === '插入末尾') {
        const currentEditor = vscode.window.activeTextEditor;
        const docToInsert = (currentEditor && currentEditor.document.uri.toString() === initialDocUri)
          ? currentEditor.document
          : targetDoc;
        const editorToInsert = (currentEditor && currentEditor.document.uri.toString() === initialDocUri)
          ? currentEditor
          : await vscode.window.showTextDocument(docToInsert);
        await editorToInsert.edit(editBuilder =>
          editBuilder.insert(docToInsert.positionAt(docToInsert.getText().length), `\n\n${output}\n`)
        );
      } else if (choice === l10n('btnViewOnly', 'View Only') || choice === '仅查看') {
        const doc = await vscode.workspace.openTextDocument({ content: output, language: 'markdown' });
        await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
      }
    } catch (err: any) { vscode.window.showErrorMessage(l10n('aiWorkbenchFailed', `ReadMD AI Workbench failed: ${errorText(err)}`, { error: errorText(err) })); }
  });
  const openSkillByUriDisposable = vscode.commands.registerCommand('readmd.openSkillByUri', async (uri?: string) => {
    if (!trusted()) return;
    if (!uri) return;
    try {
      const text = await bridge.readSkill(uri);
      const doc = await vscode.workspace.openTextDocument({ content: text, language: 'markdown' });
      await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
    } catch (err: any) { vscode.window.showErrorMessage(l10n('readSkillFailed', `Failed to read Skill: ${errorText(err)}`, { error: errorText(err) })); }
  });
  context.subscriptions.push(skillsDisposable, aiWorkbenchDisposable, openSkillByUriDisposable);

  // 1. 注册侧边栏工具箱视图
  const toolboxProvider = new ReadMDToolboxProvider(() => bridge.listSkills());
  context.subscriptions.push(toolboxProvider, vscode.window.registerTreeDataProvider('readmdToolbox', toolboxProvider));

  // 2. 状态栏指示器
  diagnosticStatusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 100);
  diagnosticStatusBarItem.command = 'readmd.fixCurrentDocument';
  context.subscriptions.push(diagnosticStatusBarItem);

  const updateStatusBar = () => {
    const editor = vscode.window.activeTextEditor;
    if (editor && (editor.document.languageId === 'markdown' || editor.document.fileName.endsWith('.md'))) {
      diagnosticStatusBarItem.text = `$(wrench) ${l10n('statusRepair', 'ReadMD Repair')}`;
      diagnosticStatusBarItem.tooltip = l10n('statusRepairTip', "Diagnose and repair Markdown formatting");
      diagnosticStatusBarItem.show();
    } else {
      diagnosticStatusBarItem.hide();
    }
  };

  vscode.window.onDidChangeActiveTextEditor(updateStatusBar, null, context.subscriptions);
  updateStatusBar();

  // Core 连接状态指示器：断线时提示，重连成功后自动隐藏
  coreStatusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Right, 99);
  coreStatusBarItem.name = 'ReadMD Core';
  context.subscriptions.push(
    coreStatusBarItem,
    bridge.onDisconnected(() => {
      coreWasDown = true;
      coreStatusBarItem.text = `$(plug) ${l10n('statusDisconnected', 'ReadMD Core disconnected')}`;
      coreStatusBarItem.tooltip = l10n('statusReconnectTip', "ReadMD will reconnect on the next operation");
      coreStatusBarItem.show();
    }),
    bridge.onReady(() => {
      coreStatusBarItem.hide();
      if (coreWasDown) {
        coreWasDown = false;
        void vscode.window.showInformationMessage(l10n('coreReconnected', 'ReadMD Core reconnected'));
      }
    })
  );

  // 3. 命令：实时双向同步增强预览 (含 KaTeX、Mermaid、WaveDrom、代码高亮)
  const previewDisposable = vscode.commands.registerCommand('readmd.preview', () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
      vscode.window.showInformationMessage(l10n('openDocFirst', 'Please open a Markdown document first'));
      return;
    }

    const key = editor.document.uri.toString();
    const existing = previews.get(key);
    if (existing) { existing.reveal(vscode.ViewColumn.Beside); return; }
    const docDir = path.dirname(editor.document.fileName);
    const localRoots = resourceRoots(docDir);

    const panel = vscode.window.createWebviewPanel(
      'readmdPreview',
      `ReadMD: ${path.basename(editor.document.fileName)}`,
      vscode.ViewColumn.Beside,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: localRoots,
      }
    );
    previews.set(key, panel);

    const sendUpdate = () => {
      const text = editor.document.getText();
      if (typeof panel.webview.postMessage === 'function') {
        panel.webview.postMessage({ type: 'updateContent', markdown: text });
      }
    };

    const initialText = editor.document.getText();
    panel.webview.html = getEnhancedWebviewContent(initialText, path.basename(editor.document.fileName), webviewAssets(panel.webview, docDir));
    let scrollSyncUntil = 0;
    const scrollSubscription = vscode.window.onDidChangeTextEditorVisibleRanges?.(event => {
      if (event.textEditor !== editor || Date.now() < scrollSyncUntil) return;
      const first = event.visibleRanges[0];
      if (!first) return;
      const ratio = first.start.line / Math.max(1, editor.document.lineCount - (first.end.line - first.start.line));
      void panel.webview.postMessage({ type: 'scrollToRatio', ratio });
    });

    if (typeof panel.webview.onDidReceiveMessage === 'function') {
      panel.webview.onDidReceiveMessage(message => {
        void handleNativeDiagram(panel.webview, message);
        if (message && message.type === 'webviewReady') {
          sendUpdate();
        }
        if (message?.type === 'openLink' && typeof message.href === 'string' && /^(https?:\/\/|mailto:)/i.test(message.href)) {
          void vscode.env.openExternal(vscode.Uri.parse(message.href));
        }
        if (message?.type === 'openDocument' && typeof message.href === 'string') void openDocumentLink(message.href, docDir);
        if (message?.type === 'previewScroll' && typeof message.ratio === 'number' && Number.isFinite(message.ratio)) {
          const line = Math.round(Math.max(0, Math.min(1, message.ratio)) * Math.max(0, editor.document.lineCount - 1));
          scrollSyncUntil = Date.now() + 350;
          editor.revealRange(new vscode.Range(line, 0, line, 0), vscode.TextEditorRevealType.AtTop);
        }
      }, null, context.subscriptions);
    }

    let debounceTimer: NodeJS.Timeout | undefined;
    const changeDocSubscription = vscode.workspace.onDidChangeTextDocument(e => {
      if (e.document.uri.toString() === editor.document.uri.toString()) {
        if (debounceTimer) clearTimeout(debounceTimer);
        debounceTimer = setTimeout(() => {
          sendUpdate();
        }, 200);
      }
    });
    const configSubscription = vscode.workspace.onDidChangeConfiguration?.(event => {
      if (!event.affectsConfiguration('readmd.customCssPath')) return;
      panel.webview.options = { ...panel.webview.options, localResourceRoots: resourceRoots(docDir) };
      panel.webview.html = getEnhancedWebviewContent(editor.document.getText(), path.basename(editor.document.fileName), webviewAssets(panel.webview, docDir));
    });

    panel.onDidDispose(() => {
      previews.delete(key);
      if (debounceTimer) clearTimeout(debounceTimer);
      changeDocSubscription.dispose();
      scrollSubscription?.dispose();
      configSubscription?.dispose();
    }, null, context.subscriptions);
  });

  // 4. 命令：智能自愈修复当前文档
  const fixDisposable = vscode.commands.registerCommand('readmd.fixCurrentDocument', async () => {
    if (!trusted()) return;
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
      vscode.window.showWarningMessage(l10n('openMdToFix', 'Please open a Markdown document to fix'));
      return;
    }

    const doc = editor.document;
    const text = doc.getText();
    const initialVersion = doc.version;
    const initialUri = doc.uri.toString();

    await vscode.window.withProgress({
      location: vscode.ProgressLocation.Notification,
      title: l10n('repairingMarkdown', 'ReadMD is diagnosing and repairing Markdown…'),
      cancellable: false,
    }, async () => {
      try {
        const res = await bridge.fixMarkdown(text);
        if (doc.version !== initialVersion || doc.uri.toString() !== initialUri) {
          vscode.window.showWarningMessage(l10n('docModifiedDuringFix', 'ReadMD: Document content was modified during repair; aborted replacement to avoid overwriting changes.'));
          return;
        }
        if (res.ok && res.repaired_content && res.repaired_content !== text) {
          await editor.edit(editBuilder => {
            const fullRange = new vscode.Range(
              doc.positionAt(0),
              doc.positionAt(text.length)
            );
            editBuilder.replace(fullRange, res.repaired_content);
          });
          const detailMsg = res.fixes_count > 0 ? l10n('repairCount', '{count} corrections', { count: res.fixes_count }) : '';
          vscode.window.showInformationMessage(l10n('docSelfHealed', `ReadMD: Document formatting successfully healed! ${detailMsg}`, { detail: detailMsg }));
        } else {
          vscode.window.showInformationMessage(l10n('docAlreadyFormatted', 'ReadMD: Document formatting is clean, no syntax issues found.'));
        }
      } catch (err: any) {
        vscode.window.showErrorMessage(l10n('selfHealFailed', `ReadMD self-heal failed: ${errorText(err)}`, { error: errorText(err) }));
      }
    });
  });

  // 5. 命令：Reveal.js 全屏演说模式
  const presentationDisposable = vscode.commands.registerCommand('readmd.openPresentation', () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
      vscode.window.showWarningMessage(l10n('openSlideFirst', 'Please open a Markdown slide presentation first'));
      return;
    }

    const panel = vscode.window.createWebviewPanel(
      'readmdPresentation',
      l10n('presentationTitle', 'Presentation: {name}', { name: path.basename(editor.document.fileName) }),
      vscode.ViewColumn.Active,
      {
        enableScripts: true,
        retainContextWhenHidden: true,
        localResourceRoots: resourceRoots(path.dirname(editor.document.fileName)),
      }
    );

    const docText = editor.document.getText();
    const docTitle = path.basename(editor.document.fileName, path.extname(editor.document.fileName));
    panel.webview.html = getWebviewContent(docText, docTitle, webviewAssets(panel.webview, path.dirname(editor.document.fileName)), true);
    panel.webview.onDidReceiveMessage?.(message => {
      void handleNativeDiagram(panel.webview, message);
      if (message?.type === 'openLink' && typeof message.href === 'string' && /^(https?:\/\/|mailto:)/i.test(message.href)) void vscode.env.openExternal(vscode.Uri.parse(message.href));
      if (message?.type === 'openDocument' && typeof message.href === 'string') void openDocumentLink(message.href, path.dirname(editor.document.fileName));
    }, null, context.subscriptions);
  });

  // 6. 命令：导出演说 HTML
  const exportPresentationDisposable = vscode.commands.registerCommand('readmd.exportPresentation', async () => {
    if (!trusted()) return;
    const editor = vscode.window.activeTextEditor;
    if (!editor) return;

    const defaultUri = vscode.Uri.file(
      editor.document.fileName.replace(/\.[^/.]+$/, '') + '.slides.html'
    );

    const saveUri = await vscode.window.showSaveDialog({
      defaultUri,
      filters: { 'Reveal.js HTML Presentation': ['html'] },
      title: l10n('exportSlidesTitle', "Export Presentation HTML"),
    });

    if (!saveUri) return;

    await vscode.window.withProgress({
      location: vscode.ProgressLocation.Notification,
      title: l10n('exportingSlides', "ReadMD is exporting the presentation…"),
      cancellable: false,
    }, async () => {
      try {
        const text = editor.document.getText();
        const docTitle = path.basename(editor.document.fileName, path.extname(editor.document.fileName));
        await bridge.exportPresentation(text, saveUri.fsPath, docTitle, 'black', 'slide', true, path.dirname(editor.document.fileName));
        const openBtn = l10n('btnOpen', 'Open');
        const choice = await vscode.window.showInformationMessage(l10n('presentationExportSuccess', 'ReadMD: Presentation successfully exported!'), openBtn);
        if (choice === openBtn) {
          vscode.env.openExternal(saveUri);
        }
      } catch (err: any) {
        vscode.window.showErrorMessage(l10n('exportPresentationFailed', `Failed to export presentation: ${errorText(err)}`, { error: errorText(err) }));
      }
    });
  });

  // 7. 命令：插入 [TOC] 目录
  const insertTocDisposable = vscode.commands.registerCommand('readmd.insertToc', async () => {
    const editor = vscode.window.activeTextEditor;
    if (!editor) return;

    editor.edit(editBuilder => {
      editBuilder.insert(editor.selection.active, '\n[TOC]\n\n');
    });
    vscode.window.showInformationMessage(l10n('insertedToc', 'ReadMD: Inserted [TOC] automatic table of contents tag'));
  });

  async function getOrCreateEditor(): Promise<vscode.TextEditor | undefined> {
    let editor = vscode.window.activeTextEditor;
    if (!editor) {
      const doc = await vscode.workspace.openTextDocument({
        content: '# ' + l10n('documentTitle', 'Document Title') + '\n\n',
        language: 'markdown',
      });
      editor = await vscode.window.showTextDocument(doc);
    }
    return editor;
  }

  // 8. 命令：插入分页符
  const insertSlideDisposable = vscode.commands.registerCommand('readmd.insertSlide', async () => {
    const editor = await getOrCreateEditor();
    if (!editor) return;

    editor.edit(editBuilder => {
      editBuilder.insert(editor.selection.active, '\n<!-- slide -->\n\n');
    });
  });

  // 8.1 命令：插入交互式代码块
  const insertCodeChunkDisposable = vscode.commands.registerCommand('readmd.insertCodeChunk', async () => {
    const editor = await getOrCreateEditor();
    if (!editor) return;

    const langPick = await vscode.window.showQuickPick([
      { label: 'python', description: l10n('pythonDescription', "Python (Matplotlib and scientific computing)"), code: 'import matplotlib.pyplot as plt\nimport numpy as np\n\nx = np.linspace(0, 10, 100)\nplt.plot(x, np.sin(x), label="sin(x)")\nplt.legend()\nplt.show()', plot: true },
      { label: 'javascript', description: l10n('javascriptDescription', "JavaScript (Node.js runtime)"), code: 'const data = [10, 20, 30, 40];\nconsole.log("Sum:", data.reduce((a, b) => a + b, 0));', plot: false },
      { label: 'bash', description: l10n('bashDescription', "Bash / Shell script"), code: '#!/usr/bin/env bash\necho "Hello ReadMD Code Chunk!"', plot: false },
      { label: 'r', description: l10n('rDescription', "R statistics and plotting"), code: 'x <- seq(0, 10, by=0.1)\nplot(x, sin(x), type="l", col="blue")', plot: false },
      { label: 'go', description: l10n('goDescription', "Go source code"), code: 'package main\nimport "fmt"\nfunc main() {\n    fmt.Println("Hello ReadMD Go!")\n}', plot: false },
    ], { placeHolder: l10n('pickCodeLanguage', "Choose a code block language") });

    if (!langPick) return;
    const flags = ['cmd=true'];
    if (langPick.plot) flags.push('matplotlib=true');
    const snippet = new vscode.SnippetString(`\`\`\`${langPick.label} {${flags.join(' ')}}\n\${1:${langPick.code}}\n\`\`\`\n$0`);
    editor.insertSnippet(snippet);
  });

  // 8.2 命令：插入科学与工程图表
  const insertDiagramDisposable = vscode.commands.registerCommand('readmd.insertDiagram', async () => {
    const editor = await getOrCreateEditor();
    if (!editor) return;

    const diagramPick = await vscode.window.showQuickPick([
      { label: 'plantuml', description: l10n('plantumlDescription', "PlantUML (sequence, architecture and class diagrams)"), template: '@startuml\nautonumber\nClient -> Server: Request\nServer --> Client: Response 200 OK\n@enduml' },
      { label: 'tikz', description: l10n('tikzDescription', "TikZ / PGFPlots (LaTeX vector diagrams)"), template: '\\begin{tikzpicture}\n\\draw[thick,->] (0,0) -- (4,0) node[anchor=north west] {x};\n\\draw[thick,->] (0,0) -- (0,3) node[anchor=south east] {y};\n\\draw[red,domain=0:3.5] plot (\\x,{0.2*\\x*\\x}) node[right] {$f(x)=\\frac{1}{5}x^2$};\n\\end{tikzpicture}' },
      { label: 'wavedrom', description: l10n('wavedromDescription', "WaveDrom (digital timing diagrams)"), template: '{\n  signal: [\n    { name: "CLK",  wave: "p......" },\n    { name: "Data", wave: "x.345x.", data: ["head", "body", "tail"] },\n    { name: "Req",  wave: "0.1..0." },\n    { name: "Ack",  wave: "0..1.0." }\n  ]\n}' },
      { label: 'vega-lite', description: l10n('vegaDescription', "Vega-Lite (statistical charts)"), template: '{\n  "$schema": "https://vega.github.io/schema/vega-lite/v5.json",\n  "mark": "bar",\n  "data": { "values": [{"a": "A", "b": 28}, {"a": "B", "b": 55}] },\n  "encoding": { "x": {"field": "a", "type": "nominal"}, "y": {"field": "b", "type": "quantitative"} }\n}' },
      { label: 'graphviz', description: l10n('graphvizDescription', "Graphviz DOT (graphs and flowcharts)"), template: 'digraph G {\n  rankdir=LR;\n  node [shape=box, style=rounded];\n  Start -> Process -> End;\n}' },
      { label: 'bitfield', description: l10n('bitfieldDescription', "BitField (register and protocol layouts)"), template: '{\n  reg: [\n    {bits: 8, name: "IPO", type: 8},\n    {bits: 8, name: "Payload"},\n    {bits: 16, name: "CRC32", type: 2}\n  ]\n}' },
    ], { placeHolder: l10n('pickDiagram', "Choose a diagram type") });

    if (!diagramPick) return;
    const snippet = new vscode.SnippetString().appendText('```' + diagramPick.label + '\n').appendPlaceholder(diagramPick.template).appendText('\n```\n').appendTabstop(0);
    editor.insertSnippet(snippet);
  });

  // 8.3 命令：插入子文档引用
  const insertDocImportDisposable = vscode.commands.registerCommand('readmd.insertDocImport', async () => {
    const editor = await getOrCreateEditor();
    if (!editor) return;

    const input = await vscode.window.showInputBox({
      prompt: l10n('importPathPrompt', "Relative Markdown path, for example chapter1.md or ./sub/details.md"),
      value: 'chapter1.md'
    });
    if (!input) return;
    const snippet = new vscode.SnippetString().appendText('@import "').appendPlaceholder(input).appendText('"\n').appendTabstop(0);
    editor.insertSnippet(snippet);
  });

  // 8.4 命令：插入 Frontmatter 样式与演示元数据
  const insertFrontmatterDisposable = vscode.commands.registerCommand('readmd.insertFrontmatter', async () => {
    const editor = await getOrCreateEditor();
    if (!editor) return;

    const doc = editor.document;
    if (/^\uFEFF?---[ \t]*\r?\n/.test(doc.getText())) {
      vscode.window.showWarningMessage(l10n('frontmatterExists', 'The current document already contains Frontmatter'));
      return;
    }
    const docTitle = doc.fileName ? path.basename(doc.fileName, path.extname(doc.fileName)) : l10n('documentTitle', "Document Title");
    const frontmatter = `---\ntitle: ${JSON.stringify(docTitle)}\nauthor: ""\npresentation:\n  theme: "black"\n  transition: "slide"\n---\n\n`;
    await editor.edit(editBuilder => {
      editBuilder.insert(new vscode.Position(0, 0), frontmatter);
    });
    vscode.window.showInformationMessage(l10n('insertedFrontmatter', 'ReadMD: Inserted Frontmatter style and presentation metadata'));
  });

  // 9. 命令：展平 @import 引用
  const processImportsDisposable = vscode.commands.registerCommand('readmd.processImports', async () => {
    if (!trusted()) return;
    const editor = vscode.window.activeTextEditor;
    if (!editor) return;

    await vscode.window.withProgress({
      location: vscode.ProgressLocation.Notification,
      title: l10n('expandingImports', "ReadMD is expanding document references…"),
      cancellable: false,
    }, async () => {
      try {
        const text = editor.document.getText();
        const baseDir = path.dirname(editor.document.fileName);
        const flattened = await bridge.processImports(text, baseDir);
        const doc = await vscode.workspace.openTextDocument({
          content: flattened,
          language: 'markdown',
        });
        await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
        vscode.window.showInformationMessage(l10n('flattenModulesSuccess', 'ReadMD: Successfully compiled and flattened all @import modules!'));
      } catch (err: any) {
        vscode.window.showErrorMessage(l10n('flattenModulesFailed', `Failed to flatten modules: ${errorText(err)}`, { error: errorText(err) }));
      }
    });
  });

  // 10. 命令：安全运行代码块
  const runCodeChunkDisposable = vscode.commands.registerCommand('readmd.runCodeChunk', async () => {
    if (!trusted()) return;
    const editor = vscode.window.activeTextEditor;
    if (!editor) return;

    const selection = editor.selection;
    let codeText = editor.document.getText(selection);
    let language = 'python';

    const fullText = editor.document.getText();
    const cursorOffset = editor.document.offsetAt(selection.active);
    const selectedFence = fencedCodeAt(codeText.trim(), 0);
    const enclosingFence = fencedCodeAt(fullText, cursorOffset);
    if (selectedFence && selectedFence.to >= codeText.trim().length) {
      codeText = selectedFence.code; language = selectedFence.language;
    } else if (enclosingFence) {
      language = enclosingFence.language;
      if (!codeText.trim()) codeText = enclosingFence.code;
    }

    const langAliases: Record<string, string> = {
      py: 'python',
      js: 'javascript',
      node: 'javascript',
      sh: 'bash',
      shell: 'bash',
    };
    if (langAliases[language]) {
      language = langAliases[language];
    }

    if (!codeText.trim()) {
      vscode.window.showInformationMessage(l10n('cursorInCodeChunk', 'Please move the cursor inside a code chunk or select the code to run'));
      return;
    }

    const runLabel = l10n('runCodeConfirm', 'Run Code');
    const confirmed = await vscode.window.showWarningMessage(
      l10n('runCodeWarning', 'Run this {language} code on your computer? It can read or modify files and access the network.', { language }),
      { modal: true, detail: codeText.slice(0, 2000) }, runLabel);
    if (confirmed !== runLabel) return;

    await vscode.window.withProgress({
      location: vscode.ProgressLocation.Notification,
      title: l10n('runningCode', 'ReadMD is running the code block…'),
      cancellable: false,
    }, async () => {
      try {
        const res = await bridge.runCodeChunk(codeText, language);
        if (res.ok) {
          let msg = res.stdout ? l10n('codeOutput', 'Output:\n{output}', { output: String(res.stdout).slice(0, 4000) }) : l10n('codeNoOutput', "Code completed with no standard output");
          if (res.images && res.images.length > 0) {
            msg += '\n' + l10n('codeImages', 'Generated {count} charts', { count: res.images.length });
          }
          vscode.window.showInformationMessage(msg);
          const panel = vscode.window.createWebviewPanel('readmdCodeOutput', l10n('codeResultsTitle', 'ReadMD · Code output'), vscode.ViewColumn.Beside,
            { enableScripts: true, localResourceRoots: [vscode.Uri.file(path.join(context.extensionPath, 'media'))] });
          const block = (value: unknown) => {
            const text = String(value || '');
            const fence = '`'.repeat(Math.max(3, ...Array.from(text.matchAll(/`+/g), match => match[0].length + 1)));
            return fence + '\n' + text + '\n' + fence;
          };
          let report = '# ' + l10n('codeResultsTitle', 'ReadMD · Code output') + '\n\n' + block(res.stdout || l10n('codeNoOutput', 'Code completed with no standard output'));
          if (res.stderr) report += '\n\n## stderr\n\n' + block(res.stderr);
          for (const image of res.images || []) {
            const src = typeof image === 'string' ? image : image?.data_url;
            if (typeof src === 'string' && /^data:image\/(?:png|jpe?g|webp);base64,[a-z\d+/=\s]+$/i.test(src)) report += '\n\n![Chart](' + src + ')';
          }
          panel.webview.html = getWebviewContent(report, 'ReadMD', webviewAssets(panel.webview));
        } else {
          vscode.window.showErrorMessage(l10n('codeExecutionError', `Code execution error: ${errorText(res?.error_code || res?.error)}`, { error: errorText(res?.error_code || res?.error) }));
        }
      } catch (err: any) {
        vscode.window.showErrorMessage(l10n('runFailed', `Run failed: ${errorText(err)}`, { error: errorText(err) }));
      }
    });
  });

  // 11. 命令：排版级导出文档 (PDF / Word / HTML / LaTeX)
  const exportDisposable = vscode.commands.registerCommand('readmd.exportDocument', async () => {
    if (!trusted()) return;
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
      vscode.window.showWarningMessage(l10n('openDocToExport', 'Please open a Markdown document to export'));
      return;
    }

    const formatPick = await vscode.window.showQuickPick([
      { label: l10n('formatPdf', "$(file-pdf) PDF (.pdf)"), value: 'pdf', description: l10n('formatPdfDetail', "Print quality PDF") },
      { label: l10n('formatWord', "$(file-text) Word (.docx)"), value: 'docx', description: l10n('formatWordDetail', "Native Word equations and document styles") },
      { label: l10n('formatEpub', "$(book) EPUB (.epub)"), value: 'epub', description: l10n('formatEpubDetail', "EPUB 3 e-book with embedded local images") },
      { label: l10n('formatHtml', "$(browser) HTML (.html)"), value: 'html', description: l10n('formatHtmlDetail', "HTML with embedded formulas and themes") },
      { label: l10n('formatLatex', "$(file-code) LaTeX (.tex)"), value: 'tex', description: l10n('formatLatexDetail', "Source for pdflatex / xelatex") },
    ], { placeHolder: l10n('pickExportFormat', "Choose an export format") });

    if (!formatPick) return;

    let presets: string[];
    try { presets = formatPick.value === 'epub' ? [] : await bridge.exportPresets(); }
    catch (error) { void vscode.window.showErrorMessage(errorText(error)); return; }
    const names: Record<string, string> = { minimal: l10n('presetMinimal', 'Minimal · Everyday notes'), classic: l10n('presetClassic', 'Classic · Books and papers'), business: l10n('presetBusiness', 'Business · Reports') };
    const presetPick = formatPick.value === 'epub' ? { value: 'minimal' } : await vscode.window.showQuickPick(presets.map(value => ({ label: names[value] || value, value })), { placeHolder: l10n('pickExportPreset', "Choose a style preset") });

    if (!presetPick) return;

    const currentExt = `.${formatPick.value}`;
    const defaultUri = vscode.Uri.file(
      editor.document.fileName.replace(/\.[^/.]+$/, '') + currentExt
    );

    const saveUri = await vscode.window.showSaveDialog({
      defaultUri,
      filters: { [formatPick.label]: [formatPick.value] },
      title: l10n('exportFormatTitle', 'Export as {format}', { format: formatPick.value.toUpperCase() }),
    });

    if (!saveUri) return;

    await vscode.window.withProgress({
      location: vscode.ProgressLocation.Notification,
      title: l10n('exportingFormat', 'ReadMD is exporting {format}…', { format: formatPick.value.toUpperCase() }),
      cancellable: false,
    }, async () => {
      try {
        const text = editor.document.getText();
        const docTitle = path.basename(editor.document.fileName, path.extname(editor.document.fileName));
        if (formatPick.value === 'epub') {
          await bridge.exportEpub(text, saveUri.fsPath, docTitle, undefined, vscode.env.language, path.dirname(editor.document.fileName), true);
        } else {
          await bridge.exportDoc(text, saveUri.fsPath, formatPick.value, presetPick.value, docTitle, path.dirname(editor.document.fileName), true);
        }
        const openBtn = l10n('btnOpen', 'Open');
        const choice = await vscode.window.showInformationMessage(l10n('exportSuccess', `ReadMD: Successfully exported to ${path.basename(saveUri.fsPath)}!`, { filename: path.basename(saveUri.fsPath) }), openBtn);
        if (choice === openBtn) {
          vscode.env.openExternal(saveUri);
        }
      } catch (err: any) {
        vscode.window.showErrorMessage(l10n('exportFailed', `Export failed: ${errorText(err)}`, { error: errorText(err) }));
      }
    });
  });

  // 12. 命令：外部文件直接转 Markdown
  const convertFileDisposable = vscode.commands.registerCommand('readmd.convertFileToMarkdown', async (uri: vscode.Uri) => {
    if (!trusted()) return;
    let filePath = uri ? uri.fsPath : '';
    if (!filePath) {
      const picks = await vscode.window.showOpenDialog({
        canSelectMany: false,
        openLabel: l10n('convertOpenLabel', "Convert to Markdown"),
        filters: {
          [l10n('supportedDocuments', 'Documents, books and media')]: ['docx', 'doc', 'pdf', 'pptx', 'ppt', 'xlsx', 'xls', 'csv', 'epub', 'mobi', 'azw3', 'odt', 'ods', 'odp', 'rtf', 'tex', 'txt', 'html', 'htm', 'ipynb', 'eml', 'zip', 'png', 'jpg', 'jpeg', 'webp', 'tif', 'tiff', 'bmp', 'wav', 'mp3', 'm4a', 'mp4', 'webm'],
          [l10n('allFiles', 'All files')]: ['*'],
        },
      });
      if (picks && picks.length > 0) {
        filePath = picks[0].fsPath;
      }
    }

    if (!filePath) return;

    await vscode.window.withProgress({
      location: vscode.ProgressLocation.Notification,
      title: l10n('convertingFile', 'ReadMD is converting {name}…', { name: path.basename(filePath) }),
      cancellable: false,
    }, async () => {
      try {
        const markdown = await bridge.convertFile(filePath);
        const doc = await vscode.workspace.openTextDocument({
          content: markdown,
          language: 'markdown',
        });
        await vscode.window.showTextDocument(doc, vscode.ViewColumn.Active);
        vscode.window.showInformationMessage(l10n('convertSuccess', `ReadMD: Successfully converted ${path.basename(filePath)}!`, { filename: path.basename(filePath) }));
      } catch (err: any) {
        vscode.window.showErrorMessage(l10n('convertFailed', `Conversion failed: ${errorText(err)}`, { error: errorText(err) }));
      }
    });
  });

  const convertAnyPromptDisposable = vscode.commands.registerCommand('readmd.convertAnyFilePrompt', () =>
    vscode.commands.executeCommand('readmd.convertFileToMarkdown'));

  // 13. 命令：抓取网页为 Markdown
  const fetchWebDisposable = vscode.commands.registerCommand('readmd.fetchWebToMarkdown', async () => {
    if (!trusted()) return;
    const url = await vscode.window.showInputBox({
      prompt: l10n('webUrlPrompt', "Web page URL, for example https://example.com/article"),
      placeHolder: 'https://...',
      validateInput: (text) => {
        try { const url = new URL(text); if (['http:', 'https:'].includes(url.protocol) && url.hostname) return null; } catch {}
        return l10n('webUrlInvalid', "Enter a valid http:// or https:// URL");
      },
    });

    if (!url) return;

    await vscode.window.withProgress({
      location: vscode.ProgressLocation.Notification,
      title: l10n('extractingWeb', 'ReadMD is extracting the web page…'),
      cancellable: false,
    }, async () => {
      try {
        const res = await bridge.fetchWeb(url);
        const doc = await vscode.workspace.openTextDocument({
          content: res.markdown,
          language: 'markdown',
        });
        await vscode.window.showTextDocument(doc, vscode.ViewColumn.Active);
        vscode.window.showInformationMessage(l10n('fetchWebSuccess', `ReadMD: Successfully fetched article "${res.title || url}"!`, { title: res.title || url }));
      } catch (err: any) {
        vscode.window.showErrorMessage(l10n('fetchWebFailed', `Web fetch failed: ${errorText(err)}`, { error: errorText(err) }));
      }
    });
  });

  // 14. 命令：一键编译转学术 LaTeX
  const convertLatexDisposable = vscode.commands.registerCommand('readmd.convertToLatex', async () => {
    if (!trusted()) return;
    const editor = vscode.window.activeTextEditor;
    if (!editor) return;

    await vscode.window.withProgress({
      location: vscode.ProgressLocation.Notification,
      title: l10n('exportingLatex', "ReadMD is generating LaTeX…"),
      cancellable: false,
    }, async () => {
      try {
        const text = editor.document.getText();
        const docTitle = path.basename(editor.document.fileName, path.extname(editor.document.fileName));
        const tex = await bridge.mdToLatex(text, docTitle);
        const doc = await vscode.workspace.openTextDocument({
          content: tex,
          language: 'latex',
        });
        await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
        vscode.window.showInformationMessage(l10n('latexConvertSuccess', 'ReadMD: Successfully generated standard academic LaTeX source!'));
      } catch (err: any) {
        vscode.window.showErrorMessage(l10n('latexConvertFailed', `LaTeX conversion failed: ${errorText(err)}`, { error: errorText(err) }));
      }
    });
  });

  // 15. 命令：扫描解析 BibTeX 参考文献
  const parseBibtexDisposable = vscode.commands.registerCommand('readmd.parseBibtex', async (uri: vscode.Uri) => {
    if (!trusted()) return;
    let bibPath = uri ? uri.fsPath : '';
    if (!bibPath) {
      const bibFiles = await vscode.workspace.findFiles('**/*.bib', '**/node_modules/**', 5);
      if (bibFiles.length > 0) {
        bibPath = bibFiles[0].fsPath;
      } else {
        const picks = await vscode.window.showOpenDialog({
          canSelectMany: false,
          filters: { 'BibTeX Database': ['bib'] },
          openLabel: l10n('parseBibLabel', "Read BibTeX Database"),
        });
        if (picks && picks.length > 0) {
          bibPath = picks[0].fsPath;
        }
      }
    }

    if (!bibPath) {
      vscode.window.showInformationMessage(l10n('noBibFound', 'No .bib file found'));
      return;
    }

    try {
      const res = await bridge.parseBibtex(bibPath);
      const entries = res?.entries || res;
      const count = Object.keys(entries || {}).length;
      vscode.window.showInformationMessage(l10n('bibtexParseSuccess', `ReadMD: BibTeX database (${path.basename(bibPath)}) loaded with ${count} entries!`, { file: path.basename(bibPath), count }));
    } catch (err: any) {
      vscode.window.showErrorMessage(l10n('bibtexParseFailed', `BibTeX parsing failed: ${errorText(err)}`, { error: errorText(err) }));
    }
  });

  // 16. 命令：一键配置工作区 MCP Server
  const setupMcpDisposable = vscode.commands.registerCommand('readmd.setupMcpServer', async () => {
    if (!trusted()) return;
    const wsFolders = vscode.workspace.workspaceFolders;
    let readmdBinary: string;
    try {
      readmdBinary = await bridge.getServerCommand();
    } catch (err) {
      vscode.window.showErrorMessage(errorText(err));
      return;
    }

    const readmdServerConfig = {
      command: readmdBinary,
      args: ['--mcp'],
    };

    const choice = await vscode.window.showQuickPick([
      { label: l10n('mcpVsCodeLabel', "Configure this workspace for VS Code"), value: 'vscode' },
      { label: l10n('mcpCursorLabel', "Configure this workspace for Cursor"), value: 'cursor' },
      { label: l10n('mcpClipboardLabel', "Copy Claude Desktop configuration"), value: 'clipboard' },
    ], { placeHolder: l10n('pickMcpClient', 'Choose an MCP client') });

    if (!choice) return;

    if (choice.value === 'clipboard') {
      const clipboardConfig = {
        mcpServers: {
          readmd: readmdServerConfig,
        },
      };
      await vscode.env.clipboard.writeText(JSON.stringify(clipboardConfig, null, 2));
      vscode.window.showInformationMessage(l10n('mcpCopiedClipboard', 'ReadMD: Copied MCP configuration to clipboard. Paste it into your Claude Desktop config file!'));
      return;
    }

    if (!wsFolders || wsFolders.length === 0) {
      vscode.window.showWarningMessage(l10n('openWorkspaceFirst', 'Please open a workspace folder in VS Code first'));
      return;
    }

    const isVsCode = choice.value === 'vscode';
    const targetDirName = isVsCode ? '.vscode' : '.cursor';
    const serverKey = isVsCode ? 'servers' : 'mcpServers';

    const folder = wsFolders.length === 1 ? wsFolders[0] : (await vscode.window.showQuickPick(wsFolders.map(folder => ({
      label: folder.name, description: folder.uri.fsPath, folder,
    })), { placeHolder: l10n('pickWorkspace', 'Choose a workspace folder to configure') }))?.folder;
    if (!folder) return;
    const targetDir = path.join(folder.uri.fsPath, targetDirName);
    const targetFile = path.join(targetDir, 'mcp.json');

    try {
      if (!fs.existsSync(targetDir)) {
        fs.mkdirSync(targetDir, { recursive: true });
      }

      const snapshot = fs.existsSync(targetFile) ? fs.readFileSync(targetFile, 'utf-8') : undefined;
      if (vscode.workspace.textDocuments?.some(document => document.isDirty && document.uri.fsPath === targetFile)) throw new Error('mcp_config_changed');
      let merged: string;
      try { merged = mergeMcpConfiguration(snapshot || '{}', serverKey, isVsCode ? { type: 'stdio', ...readmdServerConfig } : readmdServerConfig); }
      catch { throw new Error(l10n('mcpInvalidConfig', 'Cannot parse {file}; existing configuration was preserved. Fix it and try again.', { file: targetFile })); }

      const tmpFile = path.join(targetDir, `.mcp.${Date.now()}.${Math.random().toString(36).slice(2, 8)}.tmp`);
      fs.writeFileSync(tmpFile, merged, { encoding: 'utf-8', flag: 'wx' });
      try {
        if ((fs.existsSync(targetFile) ? fs.readFileSync(targetFile, 'utf-8') : undefined) !== snapshot) throw new Error('mcp_config_changed');
        fs.renameSync(tmpFile, targetFile);
      } finally {
        if (fs.existsSync(tmpFile)) fs.unlinkSync(tmpFile);
      }

      vscode.window.showInformationMessage(l10n('mcpConfigSuccess', 'ReadMD: MCP configuration created at {path}', { path: targetFile }));
    } catch (err: any) {
      vscode.window.showErrorMessage(l10n('writeMcpFailed', `Failed to write MCP config: ${errorText(err)}`, { error: errorText(err) }));
    }
  });

  context.subscriptions.push(
    previewDisposable,
    fixDisposable,
    insertCodeChunkDisposable,
    insertDiagramDisposable,
    insertDocImportDisposable,
    insertFrontmatterDisposable,
    presentationDisposable,
    exportPresentationDisposable,
    insertTocDisposable,
    insertSlideDisposable,
    processImportsDisposable,
    runCodeChunkDisposable,
    exportDisposable,
    convertFileDisposable,
    convertAnyPromptDisposable,
    fetchWebDisposable,
    convertLatexDisposable,
    parseBibtexDisposable,
    setupMcpDisposable
  );
}

export function getEnhancedWebviewContent(markdownOrTitle: string, docTitle?: string, assets?: WebviewAssets): string {
  return getWebviewContent(docTitle === undefined ? '' : markdownOrTitle, docTitle ?? markdownOrTitle,
    assets || { base: 'vscode-webview://readmd/media', cspSource: 'vscode-webview:' });
}

/** Replace only the ReadMD member, preserving unrelated JSONC comments/data. */
export function mergeMcpConfiguration(text: string, serverKey: string, config: Record<string, unknown>): string {
  const parsed = parseJsoncSafely(text);
  const existing = parsed[serverKey];
  if (existing !== undefined && (!existing || typeof existing !== 'object' || Array.isArray(existing))) throw new Error('mcp_invalid_config');
  const re = /"(?:\\.|[^"\\])*"|\/\/[^\r\n]*|\/\*[\s\S]*?\*\/|[^\s{}\[\],:]+|[{}\[\],:]/g;
  const tokens: { value: string; start: number; end: number }[] = [];
  for (const match of text.matchAll(re)) {
    if (match[0].startsWith('//') || match[0].startsWith('/*')) continue;
    tokens.push({ value: match[0], start: match.index!, end: match.index! + match[0].length });
  }
  const root = tokens.findIndex(token => token.value === '{');
  function member(open: number, key: string): { first: number; last: number } | undefined {
    let i = open + 1;
    while (i < tokens.length && tokens[i].value !== '}') {
      if (tokens[i].value === ',') { i++; continue; }
      const name = JSON.parse(tokens[i++].value);
      if (tokens[i++].value !== ':') throw new Error('mcp_invalid_config');
      const first = i;
      if (tokens[i].value === '{' || tokens[i].value === '[') {
        let depth = 0;
        do { if (['{','['].includes(tokens[i].value)) depth++; if (['}',']'].includes(tokens[i].value)) depth--; i++; } while (depth && i < tokens.length);
      } else i++;
      if (name === key) return { first, last: i - 1 };
    }
    return undefined;
  }
  function setMember(open: number, key: string, value: Record<string, unknown>, indent: string): string {
    const found = member(open, key);
    const eol = text.includes('\r\n') ? '\r\n' : '\n';
    const rendered = JSON.stringify(value, null, 2).replace(/\n/g, eol + indent);
    if (found) return text.slice(0, tokens[found.first].start) + rendered + text.slice(tokens[found.last].end);
    let close = open + 1, depth = 1;
    while (depth) { if (tokens[close].value === '{') depth++; if (tokens[close].value === '}') depth--; if (depth) close++; }
    const comma = tokens[close - 1].value === '{' || tokens[close - 1].value === ',' ? '' : ',';
    return text.slice(0, tokens[close].start) + comma + eol + indent + JSON.stringify(key) + ': ' + rendered + eol + indent.slice(2) + text.slice(tokens[close].start);
  }
  const servers = member(root, serverKey);
  const output = servers ? setMember(servers.first, 'readmd', config, '    ') : setMember(root, serverKey, { readmd: config }, '  ');
  if (JSON.stringify(parseJsoncSafely(output)[serverKey]?.readmd) !== JSON.stringify(config)) throw new Error('mcp_invalid_config');
  return output.endsWith('\n') ? output : output + '\n';
}

export function deactivate() {}
