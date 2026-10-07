import * as vscode from 'vscode';
import * as path from 'path';
import { ReadMDBridge } from './bridge';
import { l10n } from './localization';

interface Inspection {
  diagnostics: { code: string; target: string; position: { line: number; column: number } }[];
  links: { kind: string; target: string; resolved_path?: string; resolved_anchor?: string; ambiguous?: boolean; position: { line: number; column: number } }[];
  truncated: boolean;
}
const messages: Record<string, string> = {
  missing_anchor: 'Heading anchor not found', missing_file: 'Local file not found',
  ambiguous_link: 'Multiple files match this link', outside_workspace: 'Link is outside the inspected workspace',
  missing_footnote: 'Footnote definition not found',
};

/** Native Problems, document links and Quick Pick; no extra editor toolbar. */
export function registerIntelligence(context: vscode.ExtensionContext, bridge: ReadMDBridge) {
  const diagnostics = vscode.languages.createDiagnosticCollection('ReadMD');
  const cache = new Map<string, { version: number; root: string; promise: Promise<Inspection> }>();
  const timers = new Map<string, ReturnType<typeof setTimeout>>();
  let disposed = false;
  let fileChangeTimer: ReturnType<typeof setTimeout> | undefined;
  const eligible = (doc: vscode.TextDocument) => vscode.workspace.isTrusted !== false
    && doc.languageId === 'markdown' && doc.uri.scheme === 'file';
  const inspect = async (doc: vscode.TextDocument): Promise<Inspection | undefined> => {
    if (!eligible(doc) || Buffer.byteLength(doc.getText(), 'utf8') > 2 * 1024 * 1024) {
      diagnostics.delete(doc.uri); return;
    }
    const key = doc.uri.toString();
    const root = vscode.workspace.getWorkspaceFolder(doc.uri)?.uri.fsPath || path.dirname(doc.uri.fsPath);
    let item = cache.get(key);
    if (!item || item.version !== doc.version || item.root !== root) {
      item = { version: doc.version, root, promise: bridge.callMcpTool('readmd_analyze_document', {
        content: doc.getText(), file_path: doc.uri.fsPath, workspace_root: root,
      }) };
      cache.set(key, item);
    }
    try {
      const result = await item.promise;
      if (disposed || cache.get(key) !== item || doc.isClosed || doc.version !== item.version) return;
      diagnostics.set(doc.uri, result.diagnostics.map(issue => {
        const line = Math.min(Math.max(0, issue.position.line - 1), doc.lineCount - 1);
        const column = Math.min(Math.max(0, issue.position.column - 1), doc.lineAt(line).text.length);
        const diagnostic = new vscode.Diagnostic(new vscode.Range(line, column, line,
          Math.min(column + 2, doc.lineAt(line).text.length)),
          l10n('inspection_' + issue.code, messages[issue.code] || 'Check link') + ': ' + issue.target,
          vscode.DiagnosticSeverity.Warning);
        diagnostic.source = 'ReadMD'; diagnostic.code = issue.code; return diagnostic;
      }));
      return result;
    } catch (error) {
      if (cache.get(key) === item) { cache.delete(key); diagnostics.delete(doc.uri); }
      throw error;
    }
  };
  const schedule = (doc: vscode.TextDocument) => {
    if (!eligible(doc)) return;
    const key = doc.uri.toString(); clearTimeout(timers.get(key));
    timers.set(key, setTimeout(() => { timers.delete(key); void inspect(doc).catch(() => {}); }, 700));
  };
  const fileChanged = (uri: vscode.Uri) => {
    const folder = vscode.workspace.getWorkspaceFolder(uri);
    if (!folder || path.relative(folder.uri.fsPath, uri.fsPath).split(/[\\/]/)
      .some(part => ['.git', 'node_modules', 'target', 'dist', 'build', 'vendor', '__pycache__'].includes(part))) return;
    // VS Code's create/delete/rename events omit changes made by other programs.
    // Invalidate in-flight results now, debounce the native reinspection.
    cache.clear(); clearTimeout(fileChangeTimer);
    fileChangeTimer = setTimeout(() => {
      for (const doc of vscode.workspace.textDocuments) schedule(doc);
    }, 250);
  };
  const watcher = vscode.workspace.createFileSystemWatcher('**/*');
  context.subscriptions.push(diagnostics,
    watcher, watcher.onDidCreate(fileChanged), watcher.onDidChange(fileChanged), watcher.onDidDelete(fileChanged),
    { dispose() { disposed = true; clearTimeout(fileChangeTimer); for (const timer of timers.values()) clearTimeout(timer); cache.clear(); } },
    vscode.workspace.onDidOpenTextDocument(schedule),
    vscode.workspace.onDidSaveTextDocument(() => { cache.clear(); for (const doc of vscode.workspace.textDocuments) schedule(doc); }),
    vscode.workspace.onDidChangeTextDocument(event => {
      if (vscode.window.activeTextEditor?.document === event.document) schedule(event.document);
    }),
    vscode.workspace.onDidCloseTextDocument(doc => {
      const key = doc.uri.toString(); clearTimeout(timers.get(key)); timers.delete(key);
      cache.delete(key); diagnostics.delete(doc.uri);
    }),
    vscode.workspace.onDidChangeWorkspaceFolders(() => { cache.clear(); diagnostics.clear(); for (const doc of vscode.workspace.textDocuments) schedule(doc); }),
    vscode.workspace.onDidCreateFiles(() => { cache.clear(); for (const doc of vscode.workspace.textDocuments) schedule(doc); }),
    vscode.workspace.onDidDeleteFiles(() => { cache.clear(); for (const doc of vscode.workspace.textDocuments) schedule(doc); }),
    vscode.workspace.onDidRenameFiles(() => { cache.clear(); for (const doc of vscode.workspace.textDocuments) schedule(doc); }),
    vscode.languages.registerDocumentLinkProvider({ language: 'markdown', scheme: 'file' }, {
      async provideDocumentLinks(doc, token) {
        if (token.isCancellationRequested) return [];
        let result: Inspection | undefined;
        try { result = await inspect(doc); } catch { return []; }
        if (!result || token.isCancellationRequested) return [];
        return result.links.filter(link => link.kind === 'wiki' && link.resolved_path && !link.ambiguous).flatMap(link => {
          const row = link.position.line - 1;
          if (row < 0 || row >= doc.lineCount) return [];
          const line = doc.lineAt(row).text, start = link.position.column - 1, end = line.indexOf(']]', start);
          if (start < 0 || end < start) return [];
          const target = vscode.Uri.file(link.resolved_path!);
          const anchor = link.resolved_anchor || link.target.split('#')[1];
          return [new vscode.DocumentLink(new vscode.Range(row, start, row, end + 2),
            anchor ? target.with({ fragment: anchor }) : target)];
        });
      },
    }),
    vscode.commands.registerCommand('readmd.inspectDocument', async () => {
      const doc = vscode.window.activeTextEditor?.document;
      if (!doc || !eligible(doc)) { void vscode.window.showInformationMessage(l10n('inspectionOpen', 'Open a Markdown file in a trusted workspace first.')); return; }
      if (Buffer.byteLength(doc.getText(), 'utf8') > 2 * 1024 * 1024) {
        void vscode.window.showInformationMessage(l10n('inspectionSize', 'Inspection supports documents up to 2 MB.')); return;
      }
      cache.delete(doc.uri.toString());
      try {
        const result = await inspect(doc);
        if (!result) { void vscode.window.showInformationMessage(l10n('inspectionChanged', 'The document changed during inspection. Run it again.')); return; }
        await vscode.commands.executeCommand('workbench.actions.view.problems');
        void vscode.window.showInformationMessage(l10n(result.truncated ? 'inspectionLimited' : 'inspectionDone',
          result.truncated ? 'Inspection reached a processing limit; some content was not checked.' : 'Document inspection complete: {count} items.',
          { count: result.diagnostics.length }));
      } catch { void vscode.window.showErrorMessage(l10n('inspectionUnavailable', 'Update ReadMD Core to use document inspection.')); }
    }),
    vscode.commands.registerCommand('readmd.searchWorkspace', async () => {
      if (vscode.workspace.isTrusted === false) { void vscode.window.showWarningMessage(l10n('trustRequired', 'Trust this workspace before using native tools, AI or code execution.')); return; }
      const roots = vscode.workspace.workspaceFolders;
      if (!roots?.length) { void vscode.window.showInformationMessage(l10n('searchOpenWorkspace', 'Open a workspace folder first.')); return; }
      const root = roots.length === 1 ? roots[0] : await vscode.window.showWorkspaceFolderPick();
      if (!root) return;
      const query = await vscode.window.showInputBox({ prompt: l10n('searchPrompt', 'Search text, "phrases", path:, title:, tag: and -exclusions'), ignoreFocusOut: true });
      if (!query?.trim()) return;
      try {
        const result = await vscode.window.withProgress({ location: vscode.ProgressLocation.Notification,
          title: l10n('searchProgress', 'Searching local Markdown files'), cancellable: true },
          (_, token) => bridge.callMcpTool('readmd_search_workspace', { workspace_root: root.uri.fsPath, query, limit: 50 }, token));
        if (!result.results.length) { void vscode.window.showInformationMessage(l10n('searchEmpty', 'No matches in the scanned files.')); return; }
        const picks = result.results.map((hit: any) => ({ label: hit.title, description: hit.relative_path, detail: hit.snippet, hit }));
        const pick = await vscode.window.showQuickPick<{ label: string; description: string; detail: string; hit: any }>(picks,
          { matchOnDescription: true, matchOnDetail: true, placeHolder: l10n(result.truncated ? 'searchLimited' : 'searchResults',
            result.truncated ? 'Results limited to 50; refine your query' : 'Select a result to open') });
        if (!pick) return;
        const doc = await vscode.workspace.openTextDocument(vscode.Uri.file(pick.hit.path));
        const editor = await vscode.window.showTextDocument(doc);
        const row = Math.min(Math.max(0, pick.hit.line - 1), doc.lineCount - 1);
        editor.selection = new vscode.Selection(row, 0, row, 0); editor.revealRange(new vscode.Range(row, 0, row, 0));
      } catch (error) {
        if (error instanceof Error && ['ai_cancelled', 'cancelled'].includes(error.message)) return;
        void vscode.window.showErrorMessage(l10n('searchUnavailable', 'Search could not finish. Check the query and update ReadMD Core.'));
      }
    }));
  for (const doc of vscode.workspace.textDocuments) schedule(doc);
}
