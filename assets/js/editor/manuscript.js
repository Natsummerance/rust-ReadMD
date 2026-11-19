'use strict';

// Bind the requested chapter to the cursor and the current editor generation.
// The review service selects siblings from the parsed working document.
const ReadMDManuscript = (() => {
  const pending = new WeakSet();
  async function run(command) {
    const select = command === 'select';
    const view = cmView, tab = getActiveTab();
    if (!view || !tab || pending.has(view) || !state.editing) return;
    const doc = view.state.doc, selection = view.state.selection.main, caret = selection.head;
    const generation = tab.editGeneration;
    const current = () => cmView === view && getActiveTab() === tab && state.editing &&
      view.state.doc === doc && tab.editGeneration === generation && view.state.selection.main.head === caret && view.state.selection.main.anchor === selection.anchor;
    pending.add(view);
    try {
      const review = window.ReadMDReview, identity = await review.capture(tab);
      if (!identity || !current()) return;
      const response = await apiFetch('/api/review', {
        method: 'POST', headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ action: select ? 'chapter_range' : 'chapter_move_plan', direction: select ? undefined : command, caret,
          document_id: identity.document_id, generation: identity.generation, review_revision: identity.review_revision })
      });
      const plan = await response.json();
      if (!current()) return;
      if (!response.ok) throw new Error(plan.error_code || 'review_persistence_failed');
      if (plan.document_id !== identity.document_id || plan.generation !== identity.generation ||
          plan.review_revision !== identity.review_revision) throw new Error('review_stale_generation');
      if (select) {
        if (!plan.chapter) { showToast(i18n.t('editor.noChapter')); return; }
        // Keep the active end inside this section when the next heading starts at to.
        view.dispatch({ selection: { anchor: plan.chapter.to, head: plan.chapter.from }, scrollIntoView: true });
        view.focus();
        return;
      }
      if (plan.edit === null) { showToast(i18n.t('editor.noSibling')); return; }
      const { from, to, insert, anchor } = plan.edit;
      const stamp = await review.captureManualRange(from, to, tab, { allowDraft: true });
      if (!current() || stamp.document_id !== identity.document_id || stamp.generation !== identity.generation ||
          stamp.review_revision !== identity.review_revision || stamp.editGeneration !== identity.editGeneration) {
        throw new Error('review_stale_generation');
      }
      await review.applyManualRange(insert, stamp, tab, current, { userEvent: 'input.chapter.move', selectionAnchor: anchor });
      if (cmView === view && getActiveTab() === tab && state.editing) view.focus();
    } catch (error) {
      if (getActiveTab() === tab) {
        const key = 'error.' + (error.code || error.message), label = i18n.t(key);
        showToast(label && label !== key ? label : i18n.t('review.persistenceFailed'));
      }
    } finally { pending.delete(view); }
  }
  return { move: run, select: () => run('select') };
})();
