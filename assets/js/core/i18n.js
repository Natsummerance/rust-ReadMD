'use strict';
/* ============================================================
   ReadMD Core - i18n Internationalization Engine
   Supports 46+ Languages, Dynamic Translation, RTL Layout & System Detection
   ============================================================ */

window.i18n = {
  currentLang: 'zh-CN',
  meta: {
    'zh-CN': {'name': '简体中文', 'native': '简体中文', 'dir': 'ltr'},
    'zh-HK': {'name': '繁体中文（香港）', 'native': '繁體中文（香港）', 'dir': 'ltr'},
    'zh-TW': {'name': '繁体中文（台湾）', 'native': '繁體中文（台灣）', 'dir': 'ltr'},
    'en': {'name': '英语', 'native': 'English', 'dir': 'ltr'},
    'ja': {'name': '日语', 'native': '日本語', 'dir': 'ltr'},
    'ko': {'name': '韩语', 'native': '한국어', 'dir': 'ltr'},
    'fr': {'name': '法语', 'native': 'Français', 'dir': 'ltr'},
    'de': {'name': '德语', 'native': 'Deutsch', 'dir': 'ltr'},
    'es': {'name': '西班牙语', 'native': 'Español', 'dir': 'ltr'},
    'pt': {'name': '葡萄牙语', 'native': 'Português', 'dir': 'ltr'},
    'ru': {'name': '俄语', 'native': 'Русский', 'dir': 'ltr'},
    'it': {'name': '意大利语', 'native': 'Italiano', 'dir': 'ltr'},
    'ar': {'name': '阿拉伯语', 'native': 'العربية', 'dir': 'rtl'},
    'he': {'name': '希伯来语', 'native': 'עברית', 'dir': 'rtl'},
    'ug': {'name': '维吾尔语', 'native': 'ئۇيغۇرچە', 'dir': 'rtl'},
    'bo': {'name': '藏语', 'native': 'བོད་སྐད།', 'dir': 'ltr'},
    'mn': {'name': '蒙古语', 'native': 'Монгол хэл', 'dir': 'ltr'},
    'th': {'name': '泰语', 'native': 'ไทย', 'dir': 'ltr'},
    'vi': {'name': '越南语', 'native': 'Tiếng Việt', 'dir': 'ltr'},
    'id': {'name': '印尼语', 'native': 'Bahasa Indonesia', 'dir': 'ltr'},
    'hi': {'name': '印地语', 'native': 'हिन्दी', 'dir': 'ltr'},
    'bn': {'name': '孟加拉语', 'native': 'বাংলা', 'dir': 'ltr'},
    'my': {'name': '缅甸语', 'native': 'မြန်မာဘာသာ', 'dir': 'ltr'},
    'lo': {'name': '老挝语', 'native': 'ພາສາລາວ', 'dir': 'ltr'},
    'km': {'name': '高棉语', 'native': 'ភាសាខ្មែរ', 'dir': 'ltr'},
    'ms': {'name': '马来语', 'native': 'Bahasa Melayu', 'dir': 'ltr'},
    'ga': {'name': '爱尔兰语', 'native': 'Gaeilge', 'dir': 'ltr'},
    'da': {'name': '丹麦语', 'native': 'Dansk', 'dir': 'ltr'},
    'fi': {'name': '芬兰语', 'native': 'Suomi', 'dir': 'ltr'},
    'kg': {'name': '刚果语', 'native': 'Kikongo', 'dir': 'ltr'},
    'tl': {'name': '菲律宾语', 'native': 'Filipino', 'dir': 'ltr'},
    'no': {'name': '挪威语', 'native': 'Norsk', 'dir': 'ltr'},
    'sv': {'name': '瑞典语', 'native': 'Svenska', 'dir': 'ltr'},
    'kl': {'name': '格陵兰语', 'native': 'Kalaallisut', 'dir': 'ltr'},
    'nl': {'name': '荷兰语', 'native': 'Nederlands', 'dir': 'ltr'},
    'hr': {'name': '克罗地亚语', 'native': 'Hrvatski', 'dir': 'ltr'},
    'rw': {'name': '卢旺达语', 'native': 'Ikinyarwanda', 'dir': 'ltr'},
    'ro': {'name': '罗马尼亚语', 'native': 'Română', 'dir': 'ltr'},
    'mt': {'name': '马耳他语', 'native': 'Malti', 'dir': 'ltr'},
    'ne': {'name': '尼泊尔语', 'native': 'नेपाली', 'dir': 'ltr'},
    'eo': {'name': '世界语', 'native': 'Esperanto', 'dir': 'ltr'},
    'sl': {'name': '斯洛文尼亚语', 'native': 'Slovenščina', 'dir': 'ltr'},
    'tr': {'name': '土耳其语', 'native': 'Türkçe', 'dir': 'ltr'},
    'uk': {'name': '乌克兰语', 'native': 'Українська', 'dir': 'ltr'},
    'el': {'name': '希腊语', 'native': 'Ελληνικά', 'dir': 'ltr'},
    'hu': {'name': '匈牙利语', 'native': 'Magyar', 'dir': 'ltr'}
  },
  dict: {},
  fallbackDict: {},

  zhDefaults: {
    'app.disabled': '未开启',
    'app.enabled': '已开启',
    'app.loading': '加载中...',
    'app.success': '成功',
    'app.failed': '失败',
    'theme.auto': '跟随系统',
    'theme.light': '浅色',
    'theme.dark': '深色',
    'theme.sepia': '护眼',
    'theme.current': '主题：{name}',
    'pet.bubbleOpening': '正在为你打开 {name}...',
    'pet.bubbleOpened': '{name} 已打开！',
    'pet.bubbleConverting': '正在为你转换并打开 {name}...',
    'pet.bubbleOpeningMulti': '正在为你打开 {count} 篇文档...',
    'pet.bubbleOpenedMulti': '已全部打开 {count} 篇文档！',
    'pet.bubbleBatchConverting': '已将 {count} 个文档加入批量转换工作台',
    'pet.bubbleBatchFailed': '部分文件处理失败',
    'pet.renderer.sprite': '精灵图',
    'pet.gallery': '桌宠库',
    'pet.gallery.hermes': '伴读使者',
    'pet.gallery.companion': '伴读使者',
    'menu.checkUpdate': '检查更新',
    'menu.autoStart': '开机自启',
    'menu.assoc': '设为默认',
    'menu.clipboard': '剪贴板',
    'menu.saveAs': '另存为',
    'menu.share': '共享',
    'menu.fix': '修复详情',
    'menu.lang': '语言',
    'menu.convert': '万物转换',
    'menu.ocr': '扫描识别',
    'menu.web': '网页提取',
    'editor.table': '表格',
    'editor.bold': '加粗',
    'editor.formula': '公式',
    'editor.h2': '标题',
    'status.fixes': '修复',
    'status.noFixNeeded': '无需修正',
    'toast.openDocumentToUse': '请先打开文档后再使用该功能',
    'toast.opened': '已打开：{name}',
    'toast.openedPages': '已打开：{name} · {count} 页',
    'toast.openFailed': '无法打开：{error}',
    'toast.loadFailed': '加载失败：{error}',
    'toast.openFolderBrowserNotice': '浏览器模式下请使用“打开文件”',
    'ai.title': 'AI 助手',
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
    'ux.petPreviewHint': '角色外观预览 · 拖放文件到桌宠即可转换',
    'ux.petLive2d': 'Live2D · 独立桌面',
    'ux.petSprite': '精灵动画',
    'ux.graphSearch': '搜索节点与别名',
    'ux.labels': '显示名称',
    'ux.neighbors': '仅邻近关联',
    'ux.notes': '关联笔记列表',
    'ux.graphControls': '拖拽旋转 · 滚轮缩放 · 2D/3D切换',
    'ux.noResults': '未找到匹配节点',
    'ux.connections': '{count} 处关联',
    'ux.openNote': '打开笔记',
    'ux.loadFailed': '加载失败',
    'ux.filterLinks': '过滤双向链接...',
    'ux.linkHint': '使用 [[双链]] 建立笔记关联',
    'ux.starterSummary': '梳理文档',
    'ux.starterSummaryHint': '提炼论点、证据与结论',
    'ux.starterQuestions': '带着问题阅读',
    'ux.starterQuestionsHint': '发现疑点与值得追问的地方',
    'ux.noDocument': '自由对话 · 尚未打开文档',
    'batch.statusOk': '成功',
    'plugin.capability.ocr': '光学识别',
    'plugin.capability.table': '表格解析',
    'plugin.capability.pdf': 'PDF 解析',
    'plugin.capability.audio': '语音识别',
    'plugin.capability.web': '网页提取',
    'plugin.category.web': '网页提取',
    'plugin.capability.document': '文档转换',
    'plugin.capability.latex': 'LaTeX 解析',
    'plugin.capability.keywords': '关键词提取',
    'plugin.capability.highlight': '代码高亮',
    'plugin.capability.encoding': '字符编码',
    'plugin.filter.all': '全部',
    'plugin.pluggedIn': '已接通运行',
    'plugin.standby': '就绪待命',
    'plugin.exclusiveNotice': '同能力互斥',
    'plugin.requiresModel': '需模型权重',
    'plugin.switchedMutual': '已接通 {name}，同能力插件 {other} 已自动切换为休眠'
  },

  browserDefaults: {
    'app.browserCopy': 'browser copy',
  },

  /** 初始化多语言模块（毫秒级极速初始化，零阻塞启动） */
  async init() {
    // The shell is interactive before preferences finish loading. An explicit
    // choice made in that interval is newer than automatic language detection.
    const generation = this.languageEpoch || 0;
    if (generation) return;
    // 首选语言决策：1. LocalStorage 缓存 2. 系统侦测
    let preferred = null;
    try {
      preferred = localStorage.getItem('readmd_language');
    } catch (e) {}

    if (!preferred) {
      preferred = await this.detectSystemLanguage();
    }
    if ((this.languageEpoch || 0) !== generation) return;
    let language = this.matchLanguage(preferred) || 'zh-CN';
    let dict = await this.fetchDict(language);
    if (!dict && language !== 'zh-CN' && (this.languageEpoch || 0) === generation) {
      language = 'zh-CN';
      dict = await this.fetchDict(language);
    }
    if (dict && (this.languageEpoch || 0) === generation) this.applyLanguage(language, dict, false);
  },

  /** 异步载入第一兜底英文词库 */
  loadFallback() {
    if (Object.keys(this.fallbackDict).length || this.fallbackRequest) return;
    this.fallbackRequest = this.fetchDict('en').then(dict => {
      if (!dict || Object.keys(this.fallbackDict).length) return;
      this.fallbackDict = dict;
      if (this.currentLang !== 'zh-CN' && this.currentLang !== 'en') this.translateDOM(document, true);
    }).finally(() => { this.fallbackRequest = null; });
  },

  /** 高效获取语言词库 JSON */
  async fetchDict(langCode) {
    // Cache-bust: older builds served locale JSON as immutable, so a WebView
    // upgrade could keep serving a stale dictionary missing newer keys.
    const bust = `?v=${Date.now()}`;
    for (const base of ['/assets', 'assets']) {
      try {
        const resp = await fetch(`${base}/i18n/${langCode}.json${bust}`);
        if (!resp.ok) continue;
        const dict = await resp.json();
        if (dict && typeof dict === 'object' && !Array.isArray(dict) && Object.keys(dict).length) return dict;
      } catch (e) {}
    }
    return null;
  },

  matchLanguage(code) {
    if (typeof code !== 'string') return null;
    const [primary, ...parts] = code.toLowerCase().split(/[-_]/);
    if (primary === 'zh') {
      if (parts.includes('hk') || parts.includes('mo')) return 'zh-HK';
      return parts.includes('tw') || parts.includes('hant') ? 'zh-TW' : 'zh-CN';
    }
    return Object.hasOwn(this.meta, primary) ? primary : null;
  },

  /** 侦测宿主操作系统语言 */
  async detectSystemLanguage() {
    const browserLangs = navigator.languages?.length ? navigator.languages : [navigator.language];
    const browserLanguage = browserLangs.map(code => this.matchLanguage(code)).find(Boolean);
    try {
      if (typeof hasPy !== 'undefined' && hasPy && py.get_system_language) {
        const pyLang = this.matchLanguage(await py.get_system_language());
        if (pyLang) return pyLang;
      }
    } catch (e) {}
    return browserLanguage || 'zh-CN';
  },

  /** 切换语言 */
  async setLanguage(langCode, save = true) {
    const generation = this.languageEpoch = (this.languageEpoch || 0) + 1;
    langCode = this.matchLanguage(langCode) || 'zh-CN';
    const dict = await this.fetchDict(langCode);
    if (!dict || generation !== this.languageEpoch) return false;
    this.applyLanguage(langCode, dict, save);
    return true;
  },

  applyLanguage(langCode, dict, save) {
    this.dict = dict;
    this.currentLang = langCode;
    if (langCode === 'en') this.fallbackDict = dict;

    if (save) {
      try {
        localStorage.setItem('readmd_language', this.currentLang);
      } catch (e) {}
    }

    // 设置书写方向与语言标识
    const langMeta = this.meta[this.currentLang] || {};
    const dir = langMeta.dir || 'ltr';
    document.documentElement.setAttribute('dir', dir);
    document.documentElement.setAttribute('lang', this.currentLang);

    // 动态更新 DOM 文本
    this.translateDOM();
    if (typeof setEditBtn === 'function') setEditBtn(this.t(state.editing ? 'editor.editing' : 'toolbar.edit'));

    // 更新菜单中的当前语言标签
    const currentLabel = document.getElementById('lang-current-label');
    if (currentLabel) {
      currentLabel.textContent = langMeta.native || langMeta.name || this.currentLang;
    }

    // 派发切换事件
    window.dispatchEvent(new CustomEvent('readmd:language-changed', { detail: { lang: this.currentLang } }));
    if (save && window.READMD_ENGINE === 'rust' && typeof hasPy !== 'undefined' && hasPy) this.persistDesktopLanguage(langCode);
    if (langCode !== 'en' && langCode !== 'zh-CN') this.loadFallback();
  },

  persistDesktopLanguage(langCode) {
    // Keep the native profile used by desktop surfaces in the same order as
    // the renderer's language choices, even when HTTP replies are delayed.
    this.languageWrite = (this.languageWrite || Promise.resolve()).then(async () => {
      if (this.currentLang !== langCode) return;
      try {
        const response = await apiFetch('/api/system/language', {
          method: 'POST', headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ language: langCode }),
        });
        if (!response.ok || (await response.json()).applied !== true) throw new Error('language_write_failed');
      } catch (_) {
        if (this.currentLang === langCode && typeof showToast === 'function') showToast(this.t('toast.saveFailed'));
      }
    });
  },


  /** 获取翻译词条，支持 {count}, {name} 占位符替换 */
  t(key, params = {}) {
    const str = [this.dict[key], this.fallbackDict[key], this.zhDefaults[key], this.browserDefaults[key]]
      .find(value => typeof value === 'string' && value.trim() && value !== key);
    if (str === undefined) return key;
    return params ? str.replace(/\{([\w.-]+)\}/g, (match, name) => Object.hasOwn(params, name) ? String(params[name]) : match) : str;
  },

  /** 全局扫描并翻译带有 data-i18n 属性的 DOM */
  translateDOM(root = document, fallbackOnly = false) {
    const readLabel = key => {
      if (!key || fallbackOnly && typeof this.dict[key] === 'string' && this.dict[key].trim() && this.dict[key] !== key) return null;
      const label = this.t(key);
      return label === key ? null : label;
    };
    root.querySelectorAll('[data-i18n]').forEach(el => {
      const key = el.dataset.i18n, label = readLabel(key);
      if (!label) return;
      if (el.children.length === 0) {
        if (el.textContent !== label) el.textContent = label;
      } else {
        if (Array.from(el.children).some(child => child.dataset.i18n === key)) return;
        const text = Array.from(el.childNodes).find(node => node.nodeType === Node.TEXT_NODE && node.textContent.trim());
        if (!text) el.appendChild(document.createTextNode(label));
        else if (text.textContent !== label) text.textContent = label;
      }
    });

    root.querySelectorAll('[data-i18n-html]').forEach(el => {
      const label = readLabel(el.dataset.i18nHtml);
      if (label && el.innerHTML !== label) el.innerHTML = label;
    });

    for (const [suffix, attribute] of [['title', 'title'], ['placeholder', 'placeholder'], ['aria', 'aria-label'], ['label', 'label']]) {
      const source = 'data-i18n-' + suffix;
      root.querySelectorAll(`[${source}]`).forEach(el => {
        const label = readLabel(el.getAttribute(source));
        if (label && el.getAttribute(attribute) !== label) el.setAttribute(attribute, label);
      });
    }
  },


  /** 打开语言选择对话框 */
  openModal() {
    const modal = document.getElementById('lang-modal');
    if (!modal) return;
    modal.classList.remove('hidden');

    const searchInput = document.getElementById('lang-search-input');
    if (searchInput) {
      searchInput.value = '';
      setTimeout(() => {
        try { searchInput.focus(); } catch (e) {}
      }, 50);
    }
    this.renderLanguageGrid('');
  },

  /** 关闭语言选择对话框 */
  closeModal() {
    const modal = document.getElementById('lang-modal');
    if (modal) modal.classList.add('hidden');
  },

  /** 渲染语言网格列表 */
  renderLanguageGrid(query = '') {
    const grid = document.getElementById('lang-grid');
    if (!grid) return;

    grid.innerHTML = '';
    const q = query.trim().toLowerCase();

    for (const [code, info] of Object.entries(this.meta)) {
      const name = info.name || code;
      const native = info.native || name;
      if (q && !code.toLowerCase().includes(q) && !name.toLowerCase().includes(q) && !native.toLowerCase().includes(q)) {
        continue;
      }

      const item = document.createElement('button');
      item.type = 'button';
      const isActive = code === this.currentLang;
      item.className = 'lang-item' + (isActive ? ' active' : '');
      item.setAttribute('role', 'option');
      item.setAttribute('aria-selected', isActive ? 'true' : 'false');
      item.tabIndex = isActive ? 0 : -1;
      item.innerHTML = `
        <div class="lang-item-content">
          <div class="lang-item-native">${native}</div>
          <div class="lang-item-sub">${name} (${code})</div>
        </div>
        ${isActive ? '<div class="lang-item-check"><svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><polyline points="20 6 9 17 4 12"></polyline></svg></div>' : ''}
      `;
      item.addEventListener('click', async () => {
        const switching = this.setLanguage(code, true), generation = this.languageEpoch;
        if (!await switching) {
          if (generation === this.languageEpoch && typeof showToast === 'function') showToast(this.t('ux.loadFailed'));
          return;
        }
        this.closeModal();
        if (typeof showToast === 'function') {
          showToast(this.t('lang.autoDetected', { name: native }) || `Language switched to ${native}`);
        }
      });
      grid.appendChild(item);
    }

    grid.onkeydown = event => {
      const items = Array.from(grid.querySelectorAll('[role="option"]'));
      const index = items.indexOf(document.activeElement);
      if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
        event.preventDefault();
        const next = event.key === 'ArrowDown'
          ? items[(index + 1) % items.length]
          : items[(index - 1 + items.length) % items.length];
        next?.focus();
      } else if (event.key === 'Home' || event.key === 'End') {
        event.preventDefault();
        (event.key === 'Home' ? items[0] : items[items.length - 1])?.focus();
      }
    };
  }
};

document.addEventListener('keydown', event => {
  if (event.target?.id !== 'lang-search-input' || event.key !== 'ArrowDown') return;
  event.preventDefault();
  document.querySelector('#lang-grid [role="option"]')?.focus();
});
