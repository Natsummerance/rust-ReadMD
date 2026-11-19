'use strict';

let activeConfirmFinish = null;

function confirmAction({ title, message = '', confirmText, cancelText, danger = false, signal } = {}) {
  return showDecisionDialog({ title, message, confirmText, cancelText, danger, signal });
}

function promptText({ title, value = '', maxLength = 80, confirmText, cancelText } = {}) {
  return showDecisionDialog({ title, confirmText, cancelText, input: { value, maxLength } });
}

function showDecisionDialog({ title, message = '', confirmText, cancelText, danger = false, signal, input }) {
  const modal = $('confirm-modal');
  const canceled = input ? null : false;
  if (!modal || signal?.aborted) return Promise.resolve(canceled);
  activeConfirmFinish?.();

  return new Promise(resolve => {
    const confirmButton = $('confirm-action');
    const cancelButton = $('confirm-cancel');
    const field = $('confirm-input');
    let finished = false;
    const finish = value => {
      if (finished) return;
      finished = true;
      activeConfirmFinish = null;
      confirmButton.removeEventListener('click', onConfirm);
      cancelButton.removeEventListener('click', onCancel);
      field.removeEventListener('keydown', onInputKey);
      signal?.removeEventListener('abort', onAbort);
      window.ReadMDModal.close(modal);
      resolve(value);
    };
    const onConfirm = () => finish(input ? field.value : true);
    const onCancel = () => finish(canceled);
    const onAbort = onCancel;
    const onInputKey = event => {
      if (event.key !== 'Enter' || event.isComposing || event.keyCode === 229) return;
      event.preventDefault();
      onConfirm();
    };

    activeConfirmFinish = onCancel;

    $('confirm-title').textContent = title || '';
    $('confirm-message').textContent = message;
    confirmButton.textContent = confirmText || window.i18n.t('dialog.confirm');
    cancelButton.textContent = cancelText || window.i18n.t('dialog.cancel');
    confirmButton.className = danger ? 'tb-btn danger' : 'tb-btn accent';
    modal.classList.toggle('text-entry', Boolean(input));
    field.classList.toggle('hidden', !input);
    field.toggleAttribute('data-modal-initial', Boolean(input));
    cancelButton.toggleAttribute('data-modal-initial', !input);
    field.value = input?.value || '';
    field.maxLength = input?.maxLength || 80;

    confirmButton.addEventListener('click', onConfirm);
    cancelButton.addEventListener('click', onCancel);
    if (input) field.addEventListener('keydown', onInputKey);
    signal?.addEventListener('abort', onAbort, { once: true });
    // An abort may have been triggered while another confirmation was closed.
    if (signal?.aborted) { onAbort(); return; }
    window.ReadMDModal.open(modal);
  });
}
