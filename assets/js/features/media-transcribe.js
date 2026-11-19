'use strict';

(function () {
  const extensions = '.mp3,.wav,.m4a,.mp4,.flac,.ogg,.webm,.aac,.wma,.mkv,.mov,.avi';
  const text = (key, values) => window.i18n.t('mediaTranscribe.' + key, values);
  const taskKey = 'media-transcription';
  const fallbackPreviewMs = 15_000;
  let dialog, file, result, checkpoint, playback, audioContext;
  let choosing = false, working = false, saving = false, loadingDraft = false, draftRead = false;
  let activeTask, draftTimer, draftWrite, draftRevision = null, generation = 0, writtenGeneration = 0;

  const field = name => dialog.querySelector('#media-' + name);
  const setStatus = message => { field('status').textContent = message; };
  const saveError = error => {
    const message = window.i18n.t('storage.recoveryFailed', { error: errorMessage(error.message) });
    field('storage-status').textContent = message;
  };
  const basename = path => path.split(/[\\/]/).pop();
  const dirname = path => path.slice(0, path.length - basename(path).length);
  const suggested = name => name.replace(/\.[^.]+$/, '').replace(/[\\/:*?"<>|\r\n]/g, '_') || 'transcript';

  const errors = {
    cancelled: 'task.cancelled',
    file_not_found: 'error.file_not_found',
    unsupported_media_format: 'error.unsupported_format',
    ffmpeg_unavailable: 'mediaTranscribe.ffmpegMissing',
    speech_engine_unavailable: 'plugin.native.unavailable',
    speech_language_unavailable: 'convert.speechLanguage',
    speech_no_result: 'toast.ocrNoText',
    media_probe_failed: 'toast.convertFailed',
    media_decode_failed: 'toast.convertFailed',
    media_audio_empty: 'mediaTranscribe.rangeInvalid',
    media_selection_invalid: 'mediaTranscribe.rangeInvalid',
    media_audio_track_invalid: 'mediaTranscribe.rangeInvalid',
    media_range_too_long: 'mediaTranscribe.rangeInvalid',
    media_source_changed: 'mediaTranscribe.sourceChanged',
    media_busy: 'mediaTranscribe.busy',
    media_timeout: 'mediaTranscribe.timeout',
    media_recognition_failed: 'toast.convertFailed',
    media_playback_unavailable: 'export.previewFailed',
    media_subtitles_extension: 'error.unsupported_format',
    dialog_unavailable: 'error.dialog_unavailable',
    target_open_in_another_tab: 'storage.targetOpen',
    media_draft_conflict: 'error.review_save_conflict',
    media_draft_invalid: 'toast.saveFailedSimple',
    media_draft_unavailable: 'toast.saveFailedSimple',
    media_resume_conflict: 'mediaTranscribe.rangeInvalid',
    media_resume_required: 'ai.taskPaused',
  };

  function errorMessage(code) {
    const label = window.i18n.t(Object.hasOwn(errors, code) ? errors[code] : 'toast.unknownError');
    if (code === 'media_subtitles_extension') return label + ' (.vtt)';
    if (code === 'speech_language_unavailable') return label + ' · ' + window.i18n.t('plugin.native.unavailable');
    return label;
  }

  function translate() {
    if (!dialog) return;
    const t = key => window.i18n.t(key);
    window.i18n.translateDOM(dialog);
    if (!file?.media) return;
    const decoderStatus = [t('plugin.ffmpegStatus'), t('plugin.ffmpegReady')];
    if (!file.decoder.managed) decoderStatus.push('PATH');
    field('decoder').textContent = decoderStatus.join(' · ');
    const engine = file.recognizer.engine === 'whisper.cpp' ? 'whisper.cpp' : 'Windows';
    field('recognizer').textContent = file.recognizer.available
      ? [engine, t('plugin.category.audio')].join(' · ') : t('plugin.native.unavailable');
    field('range-hint').textContent = text('maxRange', { seconds: selectionLimit() / 1000 });
    field('language').options[0].textContent = t(speechAutoKey(file));
    for (const option of field('track').options) {
      option.textContent = trackLabel(file.media.tracks.find(track => track.index === Number(option.value)));
    }
  }

  function init() {
    if (dialog) return;
    dialog = document.createElement('div');
    dialog.id = 'media-transcribe-modal';
    dialog.className = 'modal-overlay hidden';
    dialog.setAttribute('role', 'dialog');
    dialog.setAttribute('aria-modal', 'true');
    dialog.setAttribute('aria-labelledby', 'media-title');
    dialog.innerHTML = `
      <section class="modal-dialog media-transcribe-box">
        <header class="modal-header">
          <h3 id="media-title" data-i18n="convert.av"></h3>
          <button id="media-close" type="button" class="tb-btn" data-modal-close data-i18n="toolbar.close"></button>
        </header>
        <div class="media-transcribe-body">
          <div class="media-transcribe-file">
            <button id="media-choose" type="button" class="tb-btn" data-modal-initial data-i18n="export.openFile"></button>
            <span id="media-filename"></span>
            <button id="media-clear-draft" type="button" class="tb-btn hidden" data-i18n="storage.delete"></button>
          </div>
          <input id="media-input" type="file" accept="${extensions}" hidden>
          <div id="media-capabilities" class="media-transcribe-hints hidden">
            <span id="media-decoder"></span>
            <span id="media-recognizer"></span>
          </div>
          <fieldset id="media-selection" class="media-transcribe-selection hidden">
            <label><span data-i18n="mediaTranscribe.audioTrack"></span><select id="media-track"></select></label>
            <label><span data-i18n="convert.speechLanguage"></span><select id="media-language"></select></label>
            <label><span data-i18n="mediaTranscribe.from"></span><input id="media-from" type="number" min="0" step=".001" aria-describedby="media-range-hint"></label>
            <label><span data-i18n="mediaTranscribe.to"></span><input id="media-to" type="number" min="0" step=".001" aria-describedby="media-range-hint"></label>
            <span id="media-range-hint"></span>
          </fieldset>
          <div id="media-result" class="media-transcribe-result hidden">
            <label for="media-cue" data-i18n="mediaTranscribe.cues"></label>
            <div class="media-transcribe-cue">
              <select id="media-cue"></select>
              <button id="media-play-cue" type="button" class="tb-btn" data-i18n="mediaTranscribe.playCue"></button>
            </div>
            <label for="media-cue-text" data-i18n="mediaTranscribe.cueText"></label>
            <textarea id="media-cue-text" rows="4" maxlength="16000"></textarea>
            <div class="media-transcribe-actions">
              <button id="media-open-markdown" type="button" class="tb-btn" data-i18n="app.open"></button>
              <button id="media-save-vtt" type="button" class="tb-btn"><span data-i18n="export.exportAction"></span> WebVTT</button>
            </div>
          </div>
          <p id="media-progress" role="status" aria-live="polite"></p>
          <p id="media-status" role="status" aria-live="polite"></p>
          <p id="media-storage-status" role="status" aria-live="polite"></p>
        </div>
        <footer class="modal-footer media-transcribe-actions">
          <button id="media-stop" type="button" class="tb-btn hidden" data-i18n="ai.stop"></button>
          <button id="media-preview" type="button" class="tb-btn hidden" data-i18n="editor.preview"></button>
          <button id="media-cancel" type="button" class="tb-btn hidden" data-i18n="task.cancel"></button>
          <button id="media-start" type="button" class="tb-btn accent hidden" data-i18n="mediaTranscribe.start"></button>
          <button id="media-resume" type="button" class="tb-btn accent hidden" data-i18n="ai.taskResume"></button>
        </footer>
      </section>`;
    document.body.append(dialog);
    field('close').addEventListener('click', close);
    field('choose').addEventListener('click', choose);
    field('clear-draft').addEventListener('click', clearDraft);
    field('input').addEventListener('change', async event => {
      const [selected] = event.target.files;
      if (!selected) return;
      await runTask(async task => {
        const path = await uploadFile(selected);
        if (path && !task.isCancelled()) await inspect(path, task.id);
      }, false);
    });
    field('track').addEventListener('change', () => { resetRange(); markDraft(); });
    for (const key of ['from', 'to', 'language']) {
      field(key).addEventListener('input', () => { updateActions(); markDraft(); });
    }
    field('preview').addEventListener('click', () => {
      const selected = selection();
      if (selected) play(file, selected);
    });
    field('stop').addEventListener('click', stopPlayback);
    field('cancel').addEventListener('click', () => {
      if (activeTask?.stepping) activeTask.stopRequested = true;
    });
    field('start').addEventListener('click', () => start(false));
    field('resume').addEventListener('click', () => start(true));
    field('cue').addEventListener('change', setCueLabel);
    field('cue-text').addEventListener('input', event => {
      const cue = currentCue();
      if (cue) { cue.text = event.target.value; markDraft(); }
    });
    field('play-cue').addEventListener('click', () => {
      const cue = currentCue();
      if (cue) play(result.file, { track: result.track, start_ms: cue.start_ms, end_ms: cue.end_ms });
    });
    field('open-markdown').addEventListener('click', openMarkdown);
    field('save-vtt').addEventListener('click', saveVtt);
    document.addEventListener('visibilitychange', () => { if (document.hidden) flushDraft(); });
    window.addEventListener('pagehide', () => { flushDraft(); });
  }

  async function open() {
    await window.__readmdPreferencesReady;
    init();
    translate();
    updateActions();
    ReadMDModal.open(dialog);
    if (!draftRead && !loadingDraft) await restoreDraft();
  }

  async function close() {
    if (working || choosing || saving || loadingDraft) return;
    stopPlayback();
    await flushDraft();
    ReadMDModal.close(dialog);
  }

  function currentRange() {
    if (!file.media) return file.range;
    return {
      track: Number(field('track').value), from: field('from').value,
      to: field('to').value, language: field('language').value,
    };
  }

  function captureDraft() {
    if (!file) return null;
    const transcript = result && Object.fromEntries(
      ['name', 'track', 'start_ms', 'end_ms', 'language', 'engine', 'cues'].map(key => [key, result[key]]),
    );
    return {
      path: file.path, source_revision: file.source_revision,
      range: currentRange(), transcript: transcript || null,
    };
  }

  function markDraft() {
    generation++;
    clearTimeout(draftTimer);
    draftTimer = setTimeout(flushDraft, 250);
  }

  function flushDraft() {
    clearTimeout(draftTimer);
    if (draftWrite) return draftWrite;
    if (writtenGeneration === generation) return Promise.resolve(true);
    draftWrite = (async () => {
      try {
        if (!draftRead) {
          const saved = await request({ operation: 'draft_load' });
          if (saved.draft) throw Error('media_draft_conflict');
          draftRevision = saved.revision;
          draftRead = true;
        }
        while (writtenGeneration !== generation) {
          const writing = generation;
          const saved = await request({
            operation: 'draft_save', revision: draftRevision, draft: captureDraft(),
          });
          draftRevision = saved.revision;
          checkpoint = saved.checkpoint || null;
          writtenGeneration = writing;
        }
        field('storage-status').textContent = file ? window.i18n.t('storage.kind.draft') : '';
        return true;
      } catch (error) {
        saveError(error);
        return false;
      }
    })().finally(() => { draftWrite = null; });
    return draftWrite;
  }

  async function restoreDraft() {
    loadingDraft = true;
    updateActions();
    try {
      const saved = await request({ operation: 'draft_load' });
      draftRevision = saved.revision;
      checkpoint = saved.checkpoint || null;
      draftRead = true;
      if (!saved.draft) return;
      const draft = saved.draft;
      // Keep corrected captions usable even when the original cannot be reopened.
      file = {
        path: draft.path, source_revision: draft.source_revision,
        range: draft.range, max_preview_ms: fallbackPreviewMs,
      };
      field('filename').textContent = basename(file.path);
      if (draft.transcript) renderCues(draft.transcript);
      field('storage-status').textContent = window.i18n.t('storage.kind.draft');
      await runTask(task => inspect(draft.path, task.id, draft));
    } catch (error) {
      saveError(error);
    } finally {
      loadingDraft = false;
      updateActions();
    }
  }

  async function clearDraft() {
    if (!file || working || choosing || saving || loadingDraft) return;
    const t = key => window.i18n.t(key);
    if (!await confirmAction({
      title: t('storage.delete'), message: t('storage.deleteConfirm'),
      confirmText: t('storage.delete'), cancelText: t('common.cancel'), danger: true,
    })) return;
    saving = true;
    updateActions();
    try {
      if (!await flushDraft()) return;
      const removed = await request({ operation: 'draft_save', revision: draftRevision, draft: null });
      draftRevision = removed.revision;
      stopPlayback();
      file = null;
      result = null;
      checkpoint = null;
      field('filename').textContent = '';
      for (const key of ['selection', 'capabilities', 'result']) field(key).classList.add('hidden');
      field('storage-status').textContent = '';
      setStatus('');
    } catch (error) {
      saveError(error);
    } finally {
      saving = false;
      updateActions();
    }
  }

  async function prepareClose() {
    if (!dialog) return true;
    const job = activeTask;
    if (job) {
      if (!job.canCancel) return false;
      if (job.stepping) job.stopRequested = true;
      if (!job.stepping || job.processing) {
        const cancelled = await ReadMDTask.requestCancel(job.id);
        if (cancelled === 'unknown' && job === activeTask && (!job.stepping || job.processing)) {
          showToast(window.i18n.t('task.cancelRetry'));
          return false;
        }
      }
      await job.done.catch(() => {});
    }
    if (choosing || saving) return false;
    await stopPlayback();
    return flushDraft();
  }

  async function choose() {
    if (working || choosing || saving || loadingDraft) return;
    if (!hasPy) {
      field('input').value = '';
      field('input').click();
      return;
    }
    choosing = true;
    updateActions();
    try {
      const path = await py.choose_any_file();
      if (path) await runTask(task => inspect(path, task.id));
    } catch (error) {
      setStatus(errorMessage(error.message));
    } finally {
      choosing = false;
      updateActions();
    }
  }

  async function request(payload) {
    const body = JSON.stringify(payload);
    const response = await apiFetch('/api/transcribe', {
      method: 'POST', headers: { 'Content-Type': 'application/json' }, body,
      keepalive: payload.operation === 'draft_save' && document.hidden && body.length < 20_000,
    });
    if (response.ok && payload.operation === 'preview') return response.arrayBuffer();
    const data = await response.json().catch(() => ({}));
    if (!response.ok || !data.ok) throw Error(data.error_code || 'media_request_failed');
    return data;
  }

  async function runTask(run, canCancel = true, stepping = false) {
    if (working) return;
    working = true;
    updateActions();
    const id = ReadMDTask.newTaskId('media');
    const job = activeTask = { id, canCancel, stepping };
    job.done = ReadMDTask.run(taskKey, context => run({
      ...context, id, stopped: () => job.stopRequested || context.isCancelled(),
      processing: running => { job.processing = running; },
    }), {
      status: field('status'),
      cancel: canCancel ? { id, button: field('cancel') } : undefined,
    });
    try {
      await job.done;
    } catch (error) {
      setStatus(errorMessage(error.message));
    } finally {
      activeTask = null;
      working = false;
      updateActions();
    }
  }

  function speechAutoKey(origin) {
    return origin.recognizer.engine === 'whisper.cpp' ? 'ocr.engineAuto' : 'convert.speechAuto';
  }

  function trackLabel(track) {
    return [text('audioTrack') + ' ' + (file.media.tracks.indexOf(track) + 1),
      track.title, track.language, track.codec].filter(Boolean).join(' · ');
  }

  async function inspect(path, taskId, restored) {
    const data = await request({
      path, operation: 'inspect', task_id: taskId, source_revision: restored?.source_revision,
    });
    const priorRange = file?.source_revision === data.source_revision ? currentRange() : null;
    stopPlayback();
    file = { ...data, path };
    if (priorRange) {
      if (result) result.file = file;
    } else {
      result = null;
      checkpoint = null;
      field('result').classList.add('hidden');
    }
    field('filename').textContent = basename(path);
    field('track').replaceChildren(...file.media.tracks.map(track => new Option(trackLabel(track), track.index)));
    const preferredTrack = file.media.tracks.find(track => track.index === priorRange?.track)
      || file.media.tracks.find(track => track.default) || file.media.tracks[0];
    field('track').value = preferredTrack.index;
    const preferred = priorRange?.language || currentSpeechLanguage();
    const languages = (file.recognizer.languages || []).filter(language => language.code !== 'auto');
    field('language').replaceChildren(new Option(window.i18n.t(speechAutoKey(file)), 'auto'),
      ...languages.map(language => new Option(language.name, language.code)));
    field('language').value = languages.find(language => language.code === preferred || language.code.split('-')[0] === preferred)?.code || 'auto';
    field('capabilities').classList.remove('hidden');
    field('selection').classList.remove('hidden');
    resetRange();
    if (preferredTrack.index === priorRange?.track) {
      field('from').value = priorRange.from;
      field('to').value = priorRange.to;
    }
    translate();
    if (!restored) { markDraft(); await flushDraft(); }
  }

  function selectionLimit() {
    return file.max_task_selection_ms ?? file.max_selection_ms;
  }

  function resetRange() {
    const track = file.media.tracks.find(track => track.index === Number(field('track').value));
    const limit = track.start_ms + selectionLimit();
    const end = Math.min(track.end_ms ?? file.media.duration_ms ?? limit, limit);
    field('from').value = track.start_ms / 1000;
    field('to').value = end / 1000;
    updateActions();
  }

  function selection() {
    if (!file?.media) return null;
    const from = Math.round(field('from').valueAsNumber * 1000);
    const to = Math.round(field('to').valueAsNumber * 1000);
    if (!Number.isSafeInteger(from) || !Number.isSafeInteger(to) || from < 0 || to <= from) return null;
    return { track: Number(field('track').value), start_ms: from, end_ms: to };
  }

  function updateActions() {
    const locked = working || choosing || saving || loadingDraft;
    const interval = selection();
    field('close').disabled = locked;
    field('choose').disabled = locked;
    field('clear-draft').classList.toggle('hidden', !file);
    field('clear-draft').disabled = locked;
    field('selection').disabled = locked;
    field('language').disabled = !file?.recognizer?.available;
    field('preview').classList.toggle('hidden', !file?.media);
    field('preview').disabled = locked || !interval;
    const available = Boolean(file?.media && file.recognizer.available);
    const paused = checkpoint && !checkpoint.done;
    const matches = paused && interval && samePlan(checkpoint, interval, field('language').value);
    field('start').classList.toggle('hidden', !available || Boolean(matches));
    field('start').disabled = locked || !interval || interval.end_ms - interval.start_ms > selectionLimit();
    field('resume').classList.toggle('hidden', !available || !paused);
    field('resume').disabled = locked;
    field('stop').classList.toggle('hidden', !playback);
    for (const key of ['cue', 'cue-text', 'play-cue', 'save-vtt', 'open-markdown']) field(key).disabled = locked;
    field('progress').textContent = paused && !working
      ? window.i18n.t('ai.taskPaused') + ' · ' + checkpointProgress(checkpoint) : '';
  }

  function samePlan(saved, interval, language) {
    return saved.language === language && ['track', 'start_ms', 'end_ms']
      .every(key => saved.selection[key] === interval[key]);
  }

  function checkpointProgress(saved) {
    const { start_ms: from, end_ms: to } = saved.selection;
    return timestamp(saved.next_ms - from, false) + ' / ' + timestamp(to - from, false);
  }

  function applyCheckpoint(data) {
    draftRevision = data.revision;
    checkpoint = data.checkpoint || null;
    if (data.draft?.transcript) renderCues(data.draft.transcript);
    field('storage-status').textContent = window.i18n.t('storage.kind.draft');
    updateActions();
  }

  async function start(resume) {
    const continuation = resume ? checkpoint : null;
    const interval = continuation?.selection || selection();
    if (!file?.recognizer?.available || !interval || working || (resume && (!continuation || continuation.done))) return;
    const language = continuation?.language || field('language').value;
    if (continuation) {
      field('track').value = interval.track;
      field('from').value = interval.start_ms / 1000;
      field('to').value = interval.end_ms / 1000;
      field('language').value = language;
      markDraft();
    }
    stopPlayback();
    await runTask(async task => {
      if (!await flushDraft()) return;
      let current = continuation;
      try {
        while (!task.stopped()) {
          task.processing(true);
          let outcome;
          try {
            outcome = await request({
              path: file.path, source_revision: file.source_revision, operation: 'transcribe_step',
              ...interval, language, task_id: task.id, draft_revision: draftRevision,
              checkpoint_id: current?.id || null, cursor_ms: current?.next_ms ?? null,
            });
          } finally {
            task.processing(false);
          }
          applyCheckpoint(outcome);
          if (!checkpoint) throw Error('media_request_failed');
          if (checkpoint.done) {
            if (!checkpoint.has_cues) setStatus(errorMessage('speech_no_result'));
            return;
          }
          if (current?.next_ms === checkpoint.next_ms) throw Error('media_resume_conflict');
          current = checkpoint;
          task.progress(checkpointProgress(current));
        }
        if (!checkpoint || checkpoint.done) setStatus(window.i18n.t('task.cancelled'));
      } catch (error) {
        try {
          const data = await request({ operation: 'draft_load' });
          if (data.draft?.path === file.path && data.draft.source_revision === file.source_revision) applyCheckpoint(data);
        } catch (_) { /* Keep already displayed committed cues. */ }
        throw error;
      }
    }, true, true);
  }

  function renderCues(data) {
    result = { ...data, file };
    field('cue').replaceChildren(...result.cues.map((cue, index) =>
      new Option(`${timestamp(cue.start_ms)} – ${timestamp(cue.end_ms)}`, index)));
    field('result').classList.remove('hidden');
    setCueLabel();
  }

  function currentCue() {
    return result?.cues[Number(field('cue').value)];
  }

  function setCueLabel() {
    field('cue-text').value = currentCue()?.text || '';
  }

  function stopPlayback() {
    if (!playback) return;
    const active = playback;
    playback = null;
    if (active.node) active.node.stop();
    updateActions();
    if (active.requested) return ReadMDTask.requestCancel(active.id);
  }

  async function play(origin, interval) {
    stopPlayback();
    const active = { id: ReadMDTask.newTaskId('media-preview'), requested: false, node: null };
    playback = active;
    updateActions();
    try {
      const Context = window.AudioContext || window.webkitAudioContext;
      if (!Context) throw Error('media_playback_unavailable');
      audioContext ||= new Context();
      await audioContext.resume();
      if (playback !== active) return;
      active.requested = true;
      const bytes = await request({
        path: origin.path, source_revision: origin.source_revision, operation: 'preview',
        ...interval, task_id: active.id,
        end_ms: Math.min(interval.end_ms, interval.start_ms + origin.max_preview_ms),
      });
      active.requested = false;
      if (playback !== active) return;
      const buffer = await audioContext.decodeAudioData(bytes);
      if (playback !== active) return;
      active.node = audioContext.createBufferSource();
      active.node.buffer = buffer;
      active.node.connect(audioContext.destination);
      active.node.addEventListener('ended', () => {
        if (playback === active) { playback = null; updateActions(); }
      }, { once: true });
      active.node.start();
    } catch (error) {
      if (playback !== active) return;
      playback = null;
      setStatus(errorMessage(error.message));
      updateActions();
    }
  }

  function timestamp(time, precision = true) {
    const digits = value => String(value).padStart(2, '0');
    const hours = Math.floor(time / 3600000);
    const short = digits(Math.floor(time / 60000) % 60) + ':' + digits(Math.floor(time / 1000) % 60);
    if (!precision && !hours) return short;
    return digits(hours) + ':' + short + (precision ? '.' + String(time % 1000).padStart(3, '0') : '');
  }

  function writtenCues() {
    return result.cues.filter(cue => cue.text.trim());
  }

  function vtt() {
    return 'WEBVTT\n\n' + writtenCues().map(cue => {
      const plain = cue.text.split(/\r\n|\r|\n/).map(line => line.trim()).filter(Boolean).join('\n');
      const safe = plain.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
      return `${timestamp(cue.start_ms)} --> ${timestamp(cue.end_ms)}\n${safe}\n\n`;
    }).join('');
  }

  async function saveVtt() {
    if (!result || saving || working) return;
    saving = true;
    updateActions();
    const content = vtt();
    const name = suggested(result.name) + '.vtt';
    try {
      if (hasPy) {
        const saved = await py.save_as(content, name, [], {
          format: 'vtt', dir: dirname(result.file.path), result: true,
          blocked_paths: [result.file.path, ...state.tabs.map(tab => tab.path)].filter(Boolean),
        });
        if (saved?.ok) setStatus(window.i18n.t('storage.saved'));
        return;
      }
      const anchor = document.createElement('a');
      anchor.href = URL.createObjectURL(new Blob([content], { type: 'text/vtt;charset=utf-8' }));
      anchor.download = name;
      anchor.click();
      setTimeout(() => URL.revokeObjectURL(anchor.href), 3000);
    } catch (error) {
      setStatus(errorMessage(error.details?.error_code || error.message));
    } finally {
      saving = false;
      updateActions();
    }
  }

  async function openMarkdown() {
    if (!result || working || saving) return;
    const escape = value => value.replace(/[\r\n]+/g, ' ').replace(/[\\`*_{}\[\]()#+.!|<>&-]/g, '\\$&');
    const {name, file: origin, start_ms, end_ms, track, language, engine} = result;
    const content = [
      `---\ntitle: ${JSON.stringify(name)}\nmedia_start_ms: ${start_ms}\nmedia_end_ms: ${end_ms}\naudio_track: ${track}\nlanguage: ${JSON.stringify(language)}\nmodel: ${JSON.stringify(engine)}\n---`,
      '# ' + escape(name),
      ...writtenCues().map(cue => '**[' + timestamp(cue.start_ms, false) + ']** ' + escape(cue.text.trim())),
    ].join('\n\n') + '\n';
    await close();
    closeConvertModal();
    await renderVirtual('media', suggested(name) + '.md', dirname(origin.path), content, [], { originPath: origin.path });
  }

  window.ReadMDMediaTranscribe = { prepareClose };
  document.getElementById('convert-media-transcribe')?.addEventListener('click', open);
  window.addEventListener('readmd:language-changed', translate);
})();
