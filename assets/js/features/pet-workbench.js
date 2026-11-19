/* ReadMD's own companion controls; no third-party character assets or hooks. */
(function () {
  'use strict';
  const FALLBACKS = {
    'ux.petSections': '桌宠设置分组',
    'ux.petCharacters': '角色库',
    'ux.petCompanion': '陪伴方式',
    'ux.petSettings': '显示与运行',
    'ux.petStyle': '选择陪伴节奏',
    'ux.petSocial': '互动陪伴',
    'ux.petQuiet': '安静阅读',
    'ux.petQuietHint': '安静模式暂停主动问候和阅读提醒；你仍可点击互动，文件处理结果也会正常提示。',
    'ux.petActions': '快捷互动',
    'ux.petHello': '打个招呼',
    'ux.petGreeting': '我在这里，陪你读完这一页。',
    'ai.title': 'AI 助手',
    'pet.gallery.hermes': '伴读使者'
  };
  const t = (key, params) => {
    const val = window.i18n?.t(key, params);
    if (val && val !== key) return val;
    return FALLBACKS[key] || key;
  };
  let gallery = { pets: [], catalog: [] }, activeTab = 'characters', initialized = false;
  let choosing = false, lastTap = 0, query = '', catalogPets = [], greetingTimer, previewObserver;
  try { window.petQuietMode = localStorage.getItem('readmd.pet.quiet') === 'true'; } catch (_) { window.petQuietMode = false; }

  function getFavorites() {
    try {
      const raw = localStorage.getItem('readmd.pet.favorites');
      if (raw) {
        const arr = JSON.parse(raw);
        if (Array.isArray(arr)) return new Set(arr);
      }
    } catch (_) {}
    return new Set();
  }

  function toggleFavorite(slug) {
    const favs = getFavorites();
    if (favs.has(slug)) {
      favs.delete(slug);
    } else {
      favs.add(slug);
    }
    try {
      localStorage.setItem('readmd.pet.favorites', JSON.stringify(Array.from(favs)));
    } catch (_) {}
    renderRoster();
  }

  async function loadCatalog() {
    try {
      const res = await fetch('/assets/pet/catalog.json');
      if (res.ok) {
        const data = await res.json();
        if (Array.isArray(data) && data.length) {
          catalogPets = data;
          renderRoster();
        }
      }
    } catch (_) {}
  }

  function previewGreeting(speech) {
    const roleLine = typeof getRoleSpecificQuote === 'function' && getRoleSpecificQuote('pokesNormal');
    const contextualLine = typeof getContextualGreeting === 'function' && getContextualGreeting();
    const line = roleLine || contextualLine || t('ux.petGreeting');
    speech.textContent = line;
    const widget = document.getElementById('readmd-pet-widget');
    if (widget && !widget.classList.contains('hidden')) window.showPetBubble?.(line, 4500, 2);
    if (matchMedia('(prefers-reduced-motion: reduce)').matches) return;

    const art = document.getElementById('pet-preview-character');
    const slug = window.getActivePetSlug?.();
    if (!art || art.classList.contains('is-live2d') || slug === 'bongocat') {
      document.getElementById('pet-preview-aura')?.animate?.([
        { transform: 'translateY(0)' }, { transform: 'translateY(-4px)' }, { transform: 'translateY(0)' }
      ], { duration: 350, easing: 'ease-out' });
      return;
    }
    clearTimeout(greetingTimer);
    window.paintPetPreview?.(art, slug, 'waving', 1);
    greetingTimer = setTimeout(() => {
      if (window.getActivePetSlug?.() === slug && !art.classList.contains('is-live2d')) window.paintPetPreview?.(art, slug);
    }, 850);
  }

  function init() {
    if (initialized) return;
    const box = document.getElementById('pet-settings-box'); if (!box) return; initialized = true;
    box.classList.add('pet-workbench');
    const tabs = document.createElement('nav');
    tabs.className = 'pet-workbench-tabs';
    tabs.setAttribute('aria-label', t('ux.petSections'));
    tabs.setAttribute('data-i18n-aria', 'ux.petSections');
    for (const [id, key] of [['characters','ux.petCharacters'],['companion','ux.petCompanion'],['settings','ux.petSettings']]) {
      const button = document.createElement('button');
      button.type = 'button';
      button.dataset.petSection = id;
      button.setAttribute('data-i18n', key);
      button.textContent = t(key);
      button.onclick = () => { activeTab = id; renderTabs(); renderRoster(); };
      tabs.appendChild(button);
    }
    box.querySelector('.pet-sheet-body').before(tabs);
    const list = box.querySelector('.apple-grouped-list');
    list.querySelectorAll('.apple-list-row').forEach(row => {
      row.dataset.petSectionContent ||= row.querySelector('#pet-bubble-toggle') ? 'companion' : 'settings';
    });
    const tools = box.querySelector('.pet-library-tools');
    const search = document.getElementById('pet-roster-search');
    search.oninput = () => { query = search.value.trim().toLocaleLowerCase(); renderRoster(); };
    const feedback = document.createElement('p');
    feedback.id = 'pet-choice-feedback';
    feedback.dataset.petSectionContent = 'characters';
    feedback.setAttribute('role', 'status');
    tools.after(feedback);

    const roster = document.createElement('section');
    roster.id = 'pet-roster';
    roster.dataset.petSectionContent = 'characters';
    feedback.after(roster);
    previewObserver = new IntersectionObserver(entries => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        previewObserver.unobserve(entry.target);
        paintPetPreview(entry.target, entry.target.dataset.previewSlug);
      }
    });

    const companion = document.createElement('section');
    companion.dataset.petSectionContent = 'companion';
    companion.className = 'pet-companion-controls';
    companion.innerHTML = `<h4 data-i18n="ux.petStyle">${t('ux.petStyle')}</h4><div class="pet-mode-buttons"><button id="pet-mode-social" type="button" data-i18n="ux.petSocial">${t('ux.petSocial')}</button><button id="pet-mode-quiet" type="button" data-i18n="ux.petQuiet">${t('ux.petQuiet')}</button></div><p data-i18n="ux.petQuietHint">${t('ux.petQuietHint')}</p><h4 data-i18n="ux.petActions">${t('ux.petActions')}</h4><div class="pet-action-buttons"><button id="pet-chat-open" type="button"><span data-i18n="ai.title">${t('ai.title')}</span> ↗</button></div>`;
    list.appendChild(companion);
    companion.querySelector('#pet-mode-social').onclick = () => quiet(false);
    companion.querySelector('#pet-mode-quiet').onclick = () => quiet(true);
    const chat = () => { window.closePetSettings?.(); window.openAiPanelWithPrompt?.(null, '', null); };
    companion.querySelector('#pet-chat-open').onclick = chat;
    const quickbar = document.querySelector('.pet-widget-quick-bar');
    for (const [id, key, icon, action] of [
      ['pet-quick-chat', 'ai.title', '<path d="M21 11.5a8.4 8.4 0 0 1-.9 3.8 8.5 8.5 0 0 1-7.6 4.7 8.4 8.4 0 0 1-3.8-.9L3 21l1.9-5.7a8.4 8.4 0 0 1-.9-3.8 8.5 8.5 0 0 1 4.7-7.6 8.4 8.4 0 0 1 3.8-.9H13a8.5 8.5 0 0 1 8 8z"/>', chat],
      ['pet-quick-quiet', 'ux.petQuiet', '<path d="M21 12.8A9 9 0 0 1 11.2 3 9 9 0 1 0 21 12.8z"/>', () => quiet(!window.petQuietMode)]
    ]) {
      if (!quickbar) break;
      const button = document.createElement('button');
      button.id = id; button.type = 'button'; button.className = 'pet-quick-btn';
      button.title = t(key); button.setAttribute('aria-label', t(key));
      button.dataset.i18nTitle = key; button.dataset.i18nAria = key;
      button.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" aria-hidden="true">${icon}</svg>`;
      button.onclick = action; quickbar.prepend(button);
    }
    const preview = box.querySelector('.pet-preview-stage');
    const greeting = document.createElement('button');
    greeting.id = 'pet-say-hello';
    greeting.type = 'button';
    greeting.className = 'tb-btn secondary';
    greeting.dataset.i18n = 'ux.petHello';
    greeting.textContent = t('ux.petHello');
    greeting.setAttribute('aria-controls', 'pet-interaction-preview');
    preview.appendChild(greeting);
    const speech = document.createElement('p');
    speech.id = 'pet-interaction-preview'; speech.setAttribute('role', 'status'); preview.appendChild(speech);
    greeting.onclick = () => previewGreeting(speech);
    const enabledRow = document.getElementById('pet-enabled')?.closest('.apple-list-row');
    if (enabledRow) { delete enabledRow.dataset.petSectionContent; preview.appendChild(enabledRow); }
    window.addEventListener('readmd:pet-message', e => {
      if (!document.getElementById('pet-settings-modal')?.classList.contains('hidden') && speech.textContent !== e.detail.text) speech.textContent = e.detail.text;
    });
    quiet(window.petQuietMode, false);
    void loadCatalog();
    renderTabs();
    renderRoster();

    // Local UI feedback only. No global keyboard hook, recording or background input collection.
    document.addEventListener('keydown', e => {
      if (e.repeat || window.petQuietMode || document.hidden || matchMedia('(prefers-reduced-motion: reduce)').matches || Date.now()-lastTap<500) return;
      const pet = document.getElementById('readmd-pet-widget');
      if (!pet || pet.classList.contains('hidden') || !e.target.matches('textarea,[contenteditable="true"],.cm-content')) return;
      lastTap = Date.now(); pet.classList.add('pet-input-react'); setTimeout(() => pet.classList.remove('pet-input-react'), 220);
    });
  }
  function quiet(value, persist = true) {
    window.petQuietMode = value;
    if (persist) { try { localStorage.setItem('readmd.pet.quiet', String(value)); } catch (_) {} void window.savePetSettings?.(); }
    if (value && typeof hidePetBubble === 'function') hidePetBubble();
    document.getElementById('readmd-pet-widget')?.classList.toggle('pet-quiet', value);
    document.getElementById('pet-mode-social')?.setAttribute('aria-pressed', String(!value));
    document.getElementById('pet-mode-quiet')?.setAttribute('aria-pressed', String(value));
    document.getElementById('pet-quick-quiet')?.setAttribute('aria-pressed', String(value));
  }
  function renderTabs() {
    const box = document.getElementById('pet-settings-box');
    if (box) box.dataset.petTab = activeTab;
    const list = document.querySelector('#pet-settings-box .apple-grouped-list');
    if (list) list.scrollTop = 0;
    document.querySelectorAll('[data-pet-section]').forEach(btn => btn.setAttribute('aria-current', btn.dataset.petSection === activeTab ? 'page' : 'false'));
    document.querySelectorAll('[data-pet-section-content]').forEach(el => el.classList.toggle('pet-section-inactive', el.dataset.petSectionContent !== activeTab));
  }
  async function choose(renderer, slug) {
    const current = petSelection();
    if (choosing || current.renderer === renderer && current.slug === slug) return;
    const previousSlug = currentActivePetSlug;
    const feedback = document.getElementById('pet-choice-feedback');
    choosing = true;
    feedback.textContent = '';
    renderRoster();
    let changedGallery = false;
    try {
      if (renderer === 'hermes-sprite') {
        const active = await petGalleryRequest('/api/pets/active', { slug, confirm: true });
        if (!active.ok) throw new Error(active.error_code || active.code || 'pet_active_failed');
        changedGallery = true;
      }
      const result = await savePetSettings({ renderer, slug });
      if (!result?.ok || result.superseded) throw new Error(result?.code || 'pet_config_failed');
    } catch (error) {
      if (changedGallery) await petGalleryRequest('/api/pets/active', { slug: previousSlug, confirm: true });
      feedback.textContent = petT('pet.configFailed', { code: error.message });
    } finally {
      await refreshPetGallery();
      choosing = false;
      renderRoster();
      focusRosterButton(slug);
    }
  }

  function focusRosterButton(slug, favorite = false) {
    const selector = favorite ? '.pet-fav-btn' : '.pet-character-card';
    [...document.querySelectorAll('#pet-roster ' + selector)]
      .find(button => (favorite ? button.dataset.favoriteSlug : button.dataset.slug) === slug)?.focus({ preventScroll: true });
  }

  function rosterItems() {
    const featured = [
      { slug: 'mochi', display_name: '糯米 / Mochi' },
      { slug: 'moss', display_name: '苔苔 / Moss' },
      { slug: 'amber', display_name: '琥珀 / Amber' },
      { slug: '' }, { slug: 'bongocat' }, { slug: 'arch-chan', renderer: 'live2d' },
    ];
    const items = new Map(featured.map(pet => [pet.slug, pet]));
    for (const pet of [...(gallery.pets || []), ...(catalogPets.length ? catalogPets : (gallery.catalog || []))]) {
      if (!items.has(pet.slug)) items.set(pet.slug, pet);
    }
    return [...items.values()];
  }

  function renderRoster() {
    const box = document.getElementById('pet-settings-box');
    const host = document.getElementById('pet-roster');
    if (!host || box.closest('.hidden') || activeTab !== 'characters') return;
    const focused = host.contains(document.activeElement) ? document.activeElement : null;
    host.setAttribute('aria-busy', String(choosing));
    box.querySelectorAll('.pet-workbench-tabs button, .pet-library-tools button').forEach(button => {
      button.disabled = choosing;
    });
    document.getElementById('pet-status-card').inert = choosing;
    previewObserver.disconnect();

    const favorites = getFavorites();
    const selection = petSelection();
    const entries = rosterItems().map(pet => ({
      slug: pet.slug,
      renderer: pet.renderer || 'hermes-sprite',
      name: petCharacterName(pet.slug, pet.display_name || pet.zh_name || pet.name),
    })).filter(pet => !query || [pet.name, pet.slug, pet.renderer].join(' ').toLocaleLowerCase().includes(query));
    entries.sort((a, b) => Number(favorites.has(b.slug)) - Number(favorites.has(a.slug)));
    const cards = document.createDocumentFragment();
    for (const pet of entries) {
      const card = document.createElement('div');
      card.className = 'pet-roster-item';
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'pet-character-card';
      button.disabled = choosing;
      button.setAttribute('aria-pressed', String(pet.slug === selection.slug && pet.renderer === selection.renderer));
      button.dataset.renderer = pet.renderer;
      button.dataset.slug = pet.slug;
      button.onclick = () => choose(pet.renderer, pet.slug);

      const art = document.createElement('span');
      art.className = 'pet-card-art';
      art.setAttribute('aria-hidden', 'true');
      art.dataset.previewSlug = pet.slug;
      if (pet.renderer === 'live2d') art.style.backgroundImage = 'url("/assets/pet/arch-chan-avatar.png")';
      else previewObserver.observe(art);
      const name = document.createElement('strong');
      name.textContent = pet.name;
      button.append(art, name);
      if (petNeedsDesktop(pet)) {
        const desktop = document.createElement('small');
        desktop.title = t('pet.runtime.desktop');
        desktop.textContent = pet.renderer === 'live2d' ? 'Live2D' : t('pet.behavior.bongo');
        button.append(desktop);
      }

      const favorite = document.createElement('button');
      favorite.type = 'button';
      favorite.className = 'pet-fav-btn';
      favorite.dataset.favoriteSlug = pet.slug;
      const selected = favorites.has(pet.slug);
      favorite.setAttribute('aria-pressed', String(selected));
      const label = t(selected ? 'pet.favorited' : 'pet.favorite');
      favorite.setAttribute('aria-label', label + ' · ' + pet.name);
      favorite.title = label;
      favorite.innerHTML = '<svg viewBox="0 0 24 24" aria-hidden="true"><polygon points="12 2 15.1 8.3 22 9.3 17 14.1 18.2 21 12 17.8 5.8 21 7 14.1 2 9.3 8.9 8.3"/></svg>';
      favorite.onclick = () => toggleFavorite(pet.slug);
      card.append(button, favorite);
      cards.append(card);
    }
    if (!entries.length) {
      const empty = document.createElement('p');
      empty.className = 'pet-roster-empty';
      empty.textContent = t('ux.noResults');
      cards.append(empty);
    }
    host.replaceChildren(cards);
    if (!choosing && focused) {
      const favorite = focused.classList.contains('pet-fav-btn');
      focusRosterButton(favorite ? focused.dataset.favoriteSlug : focused.dataset.slug, favorite);
    }
  }

  window.addEventListener('readmd:pet-state', e => {
    const savedQuiet = e.detail?.preferences?.quiet;
    if (typeof savedQuiet === 'boolean') quiet(savedQuiet, false);
    renderRoster();
  });
  window.addEventListener('readmd:pet-gallery', e => {
    gallery = e.detail;
    if (Array.isArray(gallery.catalog) && gallery.catalog.length) catalogPets = gallery.catalog;
    renderRoster();
  });
  window.addEventListener('readmd:language-changed', () => {
    updateCharacterPreview();
    renderRoster();
  });
  window.addEventListener('readmd:pet-open-settings', renderRoster);
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', init); else init();
})();
