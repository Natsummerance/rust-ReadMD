import * as vscode from 'vscode';
import { createHash, randomUUID } from 'crypto';
import { ReadMDBridge } from './bridge';
import { l10n } from './localization';

/** Native diff/undo with the desktop's generator; no duplicate business logic. */
export function registerOpenAPI(context: vscode.ExtensionContext, bridge: ReadMDBridge): void {
  const reviews = new Map<string, string>();
  let busy = false;
  context.subscriptions.push(vscode.workspace.registerTextDocumentContentProvider('readmd-openapi-review', {
    provideTextDocumentContent: uri => reviews.get(uri.toString()) || '',
  }), vscode.workspace.onDidCloseTextDocument(document => reviews.delete(document.uri.toString())));
  const digest = (text: string) => createHash('sha256').update(text).digest('hex');
  context.subscriptions.push(vscode.commands.registerCommand('readmd.openApiDocument', async (source?: vscode.Uri) => {
    if (busy) return;
    if (vscode.workspace.isTrusted === false) {
      void vscode.window.showWarningMessage(l10n('trustRequired', 'Trust this workspace before using native tools, AI or code execution.')); return;
    }
    const editor = vscode.window.activeTextEditor;
    if (!editor || editor.document.languageId !== 'markdown') {
      void vscode.window.showWarningMessage(l10n('openApiOpenMarkdown', 'Open the target Markdown document first.')); return;
    }
    busy = true;
    try {
      const document = editor.document, content = document.getText(), version = document.version;
      if (Buffer.byteLength(content) > 8 * 1024 * 1024) throw Error('openapi_budget');
      if (!source) source = (await vscode.window.showOpenDialog({ canSelectMany: false, filters: { OpenAPI: ['json', 'yaml', 'yml'] } }))?.[0];
      if (!source) return;
      if (source.scheme !== 'file') throw Error('openapi_invalid');
      const valid = () => !document.isClosed && document.version === version && document.getText() === content && vscode.workspace.isTrusted !== false;
      const language = vscode.env.language.toLowerCase();
      const candidate = language.startsWith('zh') ? (language.includes('tw') ? 'zh-TW' : language.includes('hk') ? 'zh-HK' : 'zh-CN') : language.split('-')[0];
      const supported = new Set('ar bn bo da de el en eo es fi fr ga he hi hr hu id it ja kg kl km ko lo mn ms mt my ne nl no pt ro ru rw sl sv th tl tr ug uk vi zh-CN zh-HK zh-TW'.split(' '));
      const locale = supported.has(candidate) ? candidate : 'en';
      const args = { file_path: source.fsPath, content, base_revision: digest(content), locale };
      const generated = await bridge.callMcpTool('readmd_openapi_document', args);
      if (!valid()) throw Error('transaction_conflict');
      if (!generated.result.changed) { void vscode.window.showInformationMessage(l10n('openApiUpToDate', 'API documentation is already up to date.')); return; }
      if (reviews.size >= 16) throw Error('openapi_budget');
      const uri = vscode.Uri.parse('readmd-openapi-review:/API-documentation.md?' + randomUUID());
      reviews.set(uri.toString(), generated.result.content);
      await vscode.commands.executeCommand('vscode.diff', document.uri, uri, l10n('openApiReviewTitle', 'ReadMD · Review API documentation'), { preview: true });
      const apply = l10n('openApiApply', 'Accept changes');
      const decision = await vscode.window.showInformationMessage(l10n('openApiReviewHint', 'Review {count} API changes. The original file is saved only when you save the document.', { count: generated.changes.length }), apply, l10n('cancel', 'Cancel'));
      if (decision !== apply) return;
      if (!valid()) throw Error('transaction_conflict');
      // The source may have changed while the user was reviewing the diff.
      await bridge.callMcpTool('readmd_openapi_document', { ...args, expected_source_revision: generated.source_revision });
      if (!valid()) throw Error('transaction_conflict');
      // Opening the native diff can dispose the original TextEditor even while
      // its TextDocument remains open. Acquire a live editor before applying.
      const targetEditor = await vscode.window.showTextDocument(document, { viewColumn: editor.viewColumn, preserveFocus: false });
      if (!valid()) throw Error('transaction_conflict');
      const accepted = await targetEditor.edit(builder => {
        for (const edit of generated.transaction.edits) builder.replace(new vscode.Range(document.positionAt(edit.from), document.positionAt(edit.to)), edit.insert);
      }, { undoStopBefore: true, undoStopAfter: true });
      if (!accepted) throw Error('transaction_conflict');
      if (vscode.window.activeTextEditor?.document.uri.toString() === uri.toString()) {
        await vscode.commands.executeCommand('workbench.action.closeActiveEditor');
        await vscode.window.showTextDocument(document);
      }
    } catch (error) {
      const code = error instanceof Error ? error.message : '';
      const message = code === 'transaction_conflict' ? l10n('openApiConflict', 'The document or API source changed. Generate a fresh review before accepting.')
        : code === 'openapi_local_changes' ? l10n('openApiLocalChanges', 'The generated region contains manual edits. Move your notes outside it before updating.')
        : code === 'openapi_region_invalid' ? l10n('openApiInvalidRegion', 'The generated region markers are damaged. Restore them from document history.')
        : code === 'openapi_budget' ? l10n('openApiBudget', 'The specification, document or open reviews exceed the processing limit.')
        : l10n('openApiFailed', 'API documentation could not be generated. Check the OpenAPI 3.0/3.1 JSON or YAML source and ReadMD Core.');
      void vscode.window.showErrorMessage(message);
    } finally { busy = false; }
  }));
}
