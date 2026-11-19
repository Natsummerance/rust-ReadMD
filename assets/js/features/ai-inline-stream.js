'use strict';
// Completion snapshots have their own protocol; no document mutation happens
// here. A terminal result requires a complete SSE frame and request identity.
window.ReadMDCompletionStream = (() => {
  const reasons = new Set(['provider_finished', 'ready_unit', 'cached']);
  function failure(code = 'invalid_completion_stream') {
    if (code === 'cancelled') return new DOMException('cancelled', 'AbortError');
    const error = new Error(code); error.code = code; return error;
  }
  async function revision(prefix, suffix) {
    const encoder = new TextEncoder(), length = encoder.encode(prefix).byteLength;
    const bytes = await window.crypto.subtle.digest('SHA-256', encoder.encode(length + ':' + prefix + suffix));
    return Array.from(new Uint8Array(bytes), byte => byte.toString(16).padStart(2, '0')).join('');
  }
  function text(value) {
    if (typeof value !== 'string' || value.includes('\0') || Array.from(value).length > 1000) throw failure();
  }
  function identity(value, { taskId, contextRevision }, required) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) throw failure();
    for (const [key, expected] of [['task_id', taskId], ['context_revision', contextRevision]]) {
      if ((required || value[key] !== undefined) && value[key] !== expected) throw failure();
    }
    if (value.model !== undefined && (typeof value.model !== 'string' || value.model.length > 256)) throw failure();
  }
  function result(value, options, required = false) {
    identity(value, options, required);
    if (value.ok !== true || (required || value.finish_reason !== undefined) && !reasons.has(value.finish_reason)) throw failure();
    text(value.completion);
    return value;
  }
  async function consume(response, { signal, taskId, contextRevision, onSuggestion } = {}) {
    if (signal?.aborted) throw failure('cancelled');
    if (!/^[a-zA-Z0-9_-]{1,128}$/.test(taskId || '') || !/^[a-f0-9]{64}$/.test(contextRevision || '')) throw failure();
    if (!(response.headers.get('content-type') || '').toLowerCase().includes('text/event-stream') || !response.body) throw failure();
    const reader = response.body.getReader(), decoder = new TextDecoder('utf-8', { fatal: true });
    let buffer = '', data = [], terminal = null, bytes = 0, frameUnits = 0;
    const abort = () => { void reader.cancel().catch(() => {}); };
    signal?.addEventListener('abort', abort, { once: true });
    function frame() {
      if (signal?.aborted) throw failure('cancelled');
      if (!data.length) return;
      let value;
      try { value = JSON.parse(data.join('\n')); } catch (_) { throw failure(); }
      data = []; frameUnits = 0;
      identity(value, { taskId, contextRevision }, true);
      if (value.type === 'error' || value.ok === false) {
        throw failure(/^[a-z][a-z0-9_]{0,95}$/.test(value.error_code || '') ? value.error_code : 'invalid_completion_stream');
      }
      if (value.type === 'suggestion') {
        text(value.completion); onSuggestion?.(value);
      } else if (value.type === 'done') terminal = result(value, { taskId, contextRevision }, true);
      else throw failure();
    }
    function lines(eof = false) {
      for (;;) {
        const at = buffer.search(/[\r\n]/);
        if (at < 0 || !eof && buffer[at] === '\r' && at + 1 === buffer.length) return;
        const line = buffer.slice(0, at), skip = buffer[at] === '\r' && buffer[at + 1] === '\n' ? 2 : 1;
        buffer = buffer.slice(at + skip);
        if (!line) { frame(); if (terminal) return; }
        else if (line.startsWith('data:')) {
          frameUnits += line.length;
          if (frameUnits > 32768) throw failure();
          data.push(line.slice(5).replace(/^ /, ''));
        } else if (!line.startsWith(':') && !/^(event|id|retry):/.test(line)) throw failure();
      }
    }
    try {
      while (!terminal) {
        if (signal?.aborted) throw failure('cancelled');
        const next = await reader.read();
        if (signal?.aborted) throw failure('cancelled');
        if (next.done) { buffer += decoder.decode(); lines(true); if (!terminal) throw failure('incomplete_completion_stream'); break; }
        bytes += next.value.byteLength;
        if (bytes > 2 * 1024 * 1024) throw failure();
        buffer += decoder.decode(next.value, { stream: true }); lines();
        if (buffer.length > 32768) throw failure();
      }
      if (signal?.aborted) throw failure('cancelled');
      return terminal;
    } catch (error) {
      if (signal?.aborted) throw failure('cancelled');
      if (error?.name === 'TypeError') throw failure();
      throw error;
    } finally {
      signal?.removeEventListener('abort', abort);
      await reader.cancel().catch(() => {});
      reader.releaseLock();
    }
  }
  return Object.freeze({ revision, result, consume });
})();
