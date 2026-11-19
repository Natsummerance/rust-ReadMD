import * as vscode from 'vscode';
import { createHash, randomUUID } from 'crypto';
import { ReadMDBridge } from './bridge';
import { l10n } from './localization';
import { metadataViewText, openAPIReviewProjection } from './openapi-review-projection';

/** Native diff/undo with the desktop's generator; no duplicate business logic. */
export function registerOpenAPI(context: vscode.ExtensionContext, bridge: ReadMDBridge): void {
  interface ReviewView {
    id: string;
    original: string;
    generated: string;
    projection: ReturnType<typeof openAPIReviewProjection>;
    left: vscode.Uri;
    right: vscode.Uri;
    raw: boolean;
    closed: Set<string>;
    cancelled: boolean;
    applying: boolean;
  }
  const reviews = new Map<string, ReviewView>();
  const views = new Map<string, { review: ReviewView; side: 'before' | 'after' }>();
  const changed = new vscode.EventEmitter<vscode.Uri>();
  const forget = (review: ReviewView) => {
    views.delete(review.left.toString()); views.delete(review.right.toString()); reviews.delete(review.id);
  };
  let busy = false;
  context.subscriptions.push(changed, vscode.workspace.registerTextDocumentContentProvider('readmd-openapi-review', {
    onDidChange: changed.event,
    provideTextDocumentContent: uri => {
      const owned = views.get(uri.toString());
      if (!owned) return '';
      const { review, side } = owned;
      const raw = side === 'before' ? review.original : review.generated;
      const span = review.projection?.[side];
      const key = span?.state === 'previous' ? 'openApiMetadataPrevious' : span?.state === 'updated' ? 'openApiMetadataUpdated' : 'openApiMetadataSummary';
      const fallback = span?.state === 'previous' ? 'Previous API record · Operations: {operations} · Referenced files: {references}. View raw text to inspect.'
        : span?.state === 'updated' ? 'Updated API record · Operations: {operations} · Referenced files: {references}. View raw text to inspect.'
        : 'Generated API record · Operations: {operations} · Referenced files: {references}. View raw text to inspect.';
      return review.raw ? raw : metadataViewText(raw, span, l10n(key, fallback, span ? { operations: span.operations, references: span.references } : undefined));
    },
  }), vscode.workspace.onDidCloseTextDocument(document => {
    const owned = views.get(document.uri.toString());
    if (!owned) return;
    const review = owned.review;
    review.closed.add(document.uri.toString());
    if (!review.applying) review.cancelled = true;
    if (review.closed.size === 2) forget(review);
  }), { dispose: () => { views.clear(); reviews.clear(); } });
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
    let ownedReview: ReviewView | undefined;
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
      const id = randomUUID();
      const uri = vscode.Uri.parse('readmd-openapi-review:/API-documentation.md?' + id + '&after');
      const left = vscode.Uri.parse('readmd-openapi-review:/API-documentation-before.md?' + id + '&before');
      const review: ReviewView = { id, original: content, generated: generated.result.content,
        projection: openAPIReviewProjection(content, generated.result.content, generated.transaction),
        left, right: uri, raw: false, closed: new Set(), cancelled: false, applying: false };
      ownedReview = review;
      reviews.set(id, review); views.set(left.toString(), { review, side: 'before' }); views.set(uri.toString(), { review, side: 'after' });
      const currentReview = () => reviews.get(id) === review && !review.cancelled;
      await vscode.commands.executeCommand('vscode.diff', left, uri, l10n('openApiReviewTitle', 'ReadMD · Review API documentation'), { preview: true });
      const apply = l10n('openApiApply', 'Accept changes');
      let decision: string | undefined;
      while (currentReview()) {
        const toggle = review.projection ? (review.raw ? l10n('openApiViewSummary', 'Show readable diff') : l10n('openApiViewRaw', 'View raw text')) : undefined;
        decision = await vscode.window.showInformationMessage(l10n('openApiReviewHint', 'Review {count} API changes. The original file is saved only when you save the document.', { count: generated.changes.length }),
          apply, l10n('cancel', 'Cancel'), ...(toggle ? [toggle] : []));
        if (!currentReview()) return;
        if (!toggle || decision !== toggle) break;
        review.raw = !review.raw;
        changed.fire(left); changed.fire(uri);
      }
      if (decision !== apply) return;
      if (!currentReview()) throw Error('transaction_conflict');
      if (!valid()) throw Error('transaction_conflict');
      // The source may have changed while the user was reviewing the diff.
      await bridge.callMcpTool('readmd_openapi_document', { ...args, expected_source_revision: generated.source_revision });
      if (!valid()) throw Error('transaction_conflict');
      if (!currentReview()) throw Error('transaction_conflict');
      review.applying = true;
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
      if (ownedReview) forget(ownedReview);
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
