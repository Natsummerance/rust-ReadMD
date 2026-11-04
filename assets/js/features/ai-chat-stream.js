'use strict';
// Strict consumer for the kernel's chat protocol, independent of rendering.
window.ReadMDChatStream = (() => {
  function failure(code = 'ai_invalid_response') {
    if (code === 'cancelled') return new DOMException('cancelled', 'AbortError');
    const error = new Error(code); error.code = code; return error;
  }
  async function consume(response, { signal, taskId, onDelta, onUsage } = {}) {
    if (signal?.aborted) throw failure('cancelled');
    if (!(response.headers.get('content-type') || '').toLowerCase().includes('text/event-stream') || !response.body) throw failure();
    const reader = response.body.getReader(), decoder = new TextDecoder('utf-8', { fatal: true });
    let buffer = '', data = [], started = false, done = false, usage = null, bytes = 0, units = 0;
    const aborted = () => { void reader.cancel().catch(() => {}); };
    signal?.addEventListener('abort', aborted, { once: true });
    function frame() {
      if (signal?.aborted) throw failure('cancelled');
      if (!data.length) return;
      let value;
      try { value = JSON.parse(data.join('\n')); } catch (_) { throw failure(); }
      data = [];
      if (!value || typeof value !== 'object' || Array.isArray(value)) throw failure();
      if (value.task_id !== undefined && value.task_id !== taskId) throw failure();
      if (value.type === 'error') {
        const error = failure(/^[a-z][a-z0-9_]{0,95}$/.test(value.error_code || '') ? value.error_code : 'ai_invalid_response');
        if (/^[1-5][0-9]{2}$/.test(String(value.status || ''))) error.status = Number(value.status);
        throw error;
      }
      if (value.type === 'start') {
        if (started || value.task_id !== taskId) throw failure();
        started = true; return;
      }
      if (!started) throw failure();
      if (value.type === 'delta') {
        if (typeof value.delta !== 'string' || (units += value.delta.length) > 1000000) throw failure();
        onDelta?.(value.delta);
      } else if (value.type === 'usage') {
        if (!value.usage || typeof value.usage !== 'object' || Array.isArray(value.usage)) throw failure();
        for (const key of ['prompt_tokens', 'completion_tokens', 'total_tokens']) {
          if (value.usage[key] !== undefined && (!Number.isSafeInteger(value.usage[key]) || value.usage[key] < 0)) throw failure();
        }
        usage = value.usage;
      } else if (value.type === 'done') {
        if (value.ok !== true || value.task_id !== taskId) throw failure();
        done = true;
      } else throw failure();
    }
    function lines(eof) {
      for (;;) {
        const at = buffer.search(/[\r\n]/);
        if (at < 0 || (!eof && buffer[at] === '\r' && at + 1 === buffer.length)) return;
        const line = buffer.slice(0, at), skip = buffer[at] === '\r' && buffer[at + 1] === '\n' ? 2 : 1;
        buffer = buffer.slice(at + skip);
        if (!line) { frame(); if (done) return; }
        else if (line.startsWith('data:')) data.push(line.slice(5).replace(/^ /, ''));
        else if (!line.startsWith(':') && !/^(event|id|retry):/.test(line)) throw failure();
      }
    }
    try {
      while (!done) {
        if (signal?.aborted) throw failure('cancelled');
        const next = await reader.read();
        if (signal?.aborted) throw failure('cancelled');
        if (next.done) {
          buffer += decoder.decode(); lines(true);
          if (!done) throw failure();
          break;
        }
        bytes += next.value.byteLength; if (bytes > 8 * 1024 * 1024) throw failure();
        buffer += decoder.decode(next.value, { stream: true }); lines(false);
      }
      if (signal?.aborted) throw failure('cancelled');
      if (usage) onUsage?.(usage);
      return { done: true, usage, utf16Units: units };
    } catch (error) {
      if (signal?.aborted) throw failure('cancelled');
      if (error?.name === 'TypeError') throw failure();
      throw error;
    } finally {
      signal?.removeEventListener('abort', aborted);
      await reader.cancel().catch(() => {});
      reader.releaseLock();
    }
  }
  return Object.freeze({ consume });
})();
