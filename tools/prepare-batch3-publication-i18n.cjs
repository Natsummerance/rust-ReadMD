'use strict';
// Original English/Chinese seeds plus completeness checks. Translation itself
// runs through translate-capabilities.cjs and the configured provider.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict'), crypto = require('node:crypto');
const root = path.resolve(__dirname, '..'), assetFile = path.join(root, 'assets/data/publication-metadata-i18n.json');
const specFile = path.join(root, 'tools/i18n-batch3-publication-metadata.json');
const reportDirectory = path.join(root, '.cache/capability-upgrade/translations/i18n-batch3-publication-metadata');
const asset = JSON.parse(fs.readFileSync(assetFile, 'utf8'));
const locales = Object.keys(JSON.parse(fs.readFileSync(path.join(root, 'assets/i18n/meta.json'), 'utf8')));
const hash = file => crypto.createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const messages = {
  'error.publication_metadata_action': ['Choose a supported document properties action.', '请选择受支持的文档属性操作。'],
  'error.publication_metadata_alias_conflict': ['Two metadata fields name the same property with different values. Keep one value.', '两个元信息字段指向同一属性，但内容不同。请保留一个值。'],
  'error.publication_metadata_budget': ['Publication metadata exceeds the processing limit. Shorten the fields or reduce the number of entries.', '出版元信息超出处理上限。请缩短字段或减少条目。'],
  'error.publication_metadata_clear_conflict': ['A property cannot be cleared and overridden at the same time. Choose one action.', '同一属性不能同时清除和覆盖。请选择一种操作。'],
  'error.publication_metadata_duplicate': ['The front matter contains a duplicate property. Keep each field only once.', '前置元信息包含重复属性。请让每个字段仅出现一次。'],
  'error.publication_metadata_duplicate_id': ['Metadata entries share the same stable ID. Give each author, translator and institution a unique ID.', '元信息条目使用了相同的稳定 ID。请为每位作者、译者和每个机构使用唯一 ID。'],
  'error.publication_metadata_field': ['This publication metadata field is unsupported. Check its name.', '暂不支持此出版元信息字段。请检查字段名。'],
  'error.publication_metadata_id': ['Use a nonempty stable ID with supported letters, numbers and separators.', '请输入非空的稳定 ID，并使用受支持的字母、数字和分隔符。'],
  'error.publication_metadata_identifier_role': ['This identifier role is unsupported. Check its scheme and primary identifier setting.', '暂不支持此标识角色。请检查标识类型和主标识设置。'],
  'error.publication_metadata_isbn': ['The ISBN is invalid. Check its digits and checksum.', 'ISBN 无效。请检查数字和校验位。'],
  'error.publication_metadata_language': ['Use a valid content language tag, such as en-US or zh-CN.', '请使用有效的内容语言标签，例如 en-US 或 zh-CN。'],
  'error.publication_metadata_orcid': ['The ORCID is invalid. Check its digits and checksum.', 'ORCID 无效。请检查数字和校验位。'],
  'error.publication_metadata_patch_syntax': ['This properties edit cannot safely preserve the source formatting. Edit the field in the source or simplify its YAML.', '此属性修改无法安全保留源格式。请在源码中编辑该字段，或简化其 YAML。'],
  'error.publication_metadata_primary': ['Choose only one primary identifier.', '请仅选择一个主标识。'],
  'error.publication_metadata_reference': ['An institution or funding recipient ID does not exist. Select an existing entry or add it first.', '机构或受资助作者的 ID 不存在。请选择现有条目，或先添加对应条目。'],
  'error.publication_metadata_required': ['A required publication metadata value is missing.', '缺少必填的出版元信息。'],
  'error.publication_metadata_type': ['A publication metadata value has the wrong type. Check the field and list structure.', '出版元信息值的类型不正确。请检查字段和列表结构。'],
  'error.publication_metadata_yaml': ['The front matter is not valid YAML. Check indentation, quotes and field names.', '前置元信息不是有效的 YAML。请检查缩进、引号和字段名。'],
  'error.publication_metadata_yaml_feature': ['YAML anchors, aliases and custom tags are unsupported in publication metadata.', '出版元信息暂不支持 YAML 锚点、别名和自定义标签。'],
  'error.publication_metadata_pdf': ['Publication metadata could not be safely written to the generated PDF. The existing output was preserved.', '无法将出版元信息安全写入生成的 PDF，已保留原有输出。'],
  'warn.publication_metadata_title_fallback': ['No publication title was provided. The source file name is used as the title.', '未提供出版标题，已使用源文件名作为标题。'],
  'warn.publication_metadata_label_fallback': ['A content label is unavailable in the document language. Its English label is used.', '文档语言缺少某个内容标签，已使用对应的英文标签。'],
  'error.academic_context': ['Citation options conflict or contain an invalid field. Check the author mode, locator and note context.', '引用选项存在冲突或无效字段。请检查作者模式、定位信息和脚注上下文。'],
  'error.academic_context_budget': ['A citation prefix or suffix exceeds the size limit. Shorten it before continuing.', '引用前缀或后缀超出长度上限。请缩短后继续。'],
  'error.academic_rich_budget': ['Formatted citations exceed the processing limit. Shorten the citation text or split the document.', '格式化引用超出处理上限。请缩短引用文本或拆分文档。'],
  'error.academic_rich_link': ['A citation contains an unsafe or unsupported link. Check its address.', '引用包含不安全或不受支持的链接。请检查链接地址。'],
  'error.academic_rich_spacing': ['Citation spacing must use a positive line spacing and a nonnegative entry spacing.', '引用行距必须为正整数，条目间距必须为非负整数。'],
  'error.academic_rich_unsupported': ['This citation format is unsupported. Choose another CSL style.', '暂不支持此引用格式。请选择其他 CSL 样式。'],
  'error.academic_source_note': ['A citation footnote could not be matched to its source. Refresh the preview and try again.', '无法将引用脚注匹配到原文。请刷新预览后重试。'],
  'error.academic_source_overlap': ['Citation source ranges overlap. Review the citation markup before continuing.', '引用的源码范围存在重叠。请检查引用标记后继续。'],
  'error.academic_source_range': ['A citation no longer matches its source range. Refresh the preview and select it again.', '引用已不再对应原来的源码范围。请刷新预览后重新选择。'],
  'error.academic_source_revision': ['The document changed after its citations were prepared. Refresh the preview and try again.', '引用处理后文档已发生变化。请刷新预览后重试。'],
  'error.academic_projection_slots': ['The citation display no longer matches the document. Refresh the preview and try again.', '引用显示已不再对应当前文档。请刷新预览后重试。'],
  'error.academic_tex_context': ['The selected TeX bibliography engine cannot preserve this citation combination. Use CSL export or choose another engine.', '所选 TeX 文献引擎无法完整保留此引用组合。请使用 CSL 导出或选择其他引擎。'],
  'warn.publication_pdf_light_unsupported': ['The selected font has no Light weight. Regular weight is used.', '所选字体没有 Light 字重，已使用普通字重。'],
  'warn.publication_pdf_italic_font_unavailable': ['The selected font has no italic face. Review the citation appearance in the PDF.', '所选字体没有斜体字面，请检查 PDF 中的引用外观。'],
  'warn.publication_pdf_small_caps_synthesized': ['The selected font has no native small capitals. Small capitals are synthesized from letter sizes while copied text retains its original spelling.', '所选字体没有原生小型大写，已用不同大小的字形合成；复制文本仍保留原文拼写。'],
  'error.publication_pdf_rich_format_invalid': ['A citation contains invalid text formatting. Check its style before exporting the PDF.', '引用包含无效的文本格式。请检查样式后重新导出 PDF。'],
  'warn.academic_unprintable_reference': ['This CSL style cannot print the citation author or a valid substitute. Check the reference data or choose another style.', '当前 CSL 样式无法输出此引用的作者或合法替代项。请检查文献数据或更换样式。'],
  'warn.academic_diagnostic': ['This citation needs review. Check the reference data and citation options.', '此引用需要检查。请核对文献数据和引用选项。'],
  'error.academic_affix_unsupported': ['A citation prefix or suffix uses formatting that this citation engine cannot preserve. Simplify the formatting or choose another engine.', '引用前缀或后缀包含当前引文引擎无法完整保留的格式。请简化格式或选择其他引擎。'],
  'warn.publication_metadata_date_custom_only': ['The publication date is outside the supported ISO 8601 formats. Its original text is kept in custom metadata and any front matter; the standard date field is omitted.', '出版日期不属于支持的 ISO 8601 格式。原文保留在自定义元信息及已启用的前置内容中，省略标准日期字段。'],
  'warn.publication_metadata_series_position_custom_only': ['The series number cannot be represented as an EPUB group position. Its original text is kept in custom metadata and any front matter.', '丛书序号无法表示为 EPUB 的组内顺序。原文保留在自定义元信息及已启用的前置内容中。'],
};
const labels = {
  abstract: ['Abstract', '摘要'], keywords: ['Keywords', '关键词'], funding: ['Funding', '基金'],
  corresponding: ['Corresponding author', '通讯作者'], translator: ['Translator', '译者'], publisher: ['Publisher', '出版社'],
  date: ['Date', '日期'], edition: ['Edition', '版本'], rights: ['Rights', '版权'], series: ['Series', '丛书'],
};
fs.mkdirSync(reportDirectory, {recursive: true});
if (process.argv.includes('--seed')) {
  assert(!fs.existsSync(specFile), 'Preserve the existing translation spec; seed it only once');
  const spec = Object.fromEntries(Object.entries(asset.en).map(([key, en]) => {
    assert.equal(typeof asset['zh-CN'][key], 'string', 'Missing Chinese base: ' + key);
    return [key, {en, 'zh-CN': asset['zh-CN'][key]}];
  }));
  for (const [key, [en, zh]] of Object.entries(messages)) spec[key] = {en, 'zh-CN': zh};
  for (const [key, [en, zh]] of Object.entries(labels)) spec['pm.contentLabel.' + key] = {en, 'zh-CN': zh};
  fs.writeFileSync(specFile, JSON.stringify(spec, null, 2) + '\n');
  fs.writeFileSync(path.join(reportDirectory, 'preparation.json'), JSON.stringify({baseAssetSha256: hash(assetFile), baseKeyCount: Object.keys(asset.en).length, specKeyCount: Object.keys(spec).length, locales: locales.length, humanLinguisticReviewRecorded: false}, null, 2) + '\n');
  console.log('Prepared ' + Object.keys(spec).length + ' keys for ' + locales.length + ' locales; translation remains pending.');
} else {
  const spec = JSON.parse(fs.readFileSync(specFile, 'utf8'));
  const placeholders = value => (value.match(/\{[\w.-]+\}/g) || []).sort();
  for (const [key, texts] of Object.entries(spec)) {
    assert.deepEqual(Object.keys(texts).sort(), [...locales].sort(), 'Incomplete locale coverage: ' + key);
    for (const locale of locales) {
      assert.equal(typeof texts[locale], 'string'); assert(texts[locale].trim(), 'Empty translation: ' + locale + '/' + key);
      assert.deepEqual(placeholders(texts[locale]), placeholders(texts.en), 'Placeholder mismatch: ' + locale + '/' + key);
    }
  }
  for (const [key, en] of Object.entries(asset.en)) {
    assert.equal(spec[key].en, en, 'English base changed: ' + key);
    assert.equal(spec[key]['zh-CN'], asset['zh-CN'][key], 'Chinese base changed: ' + key);
  }
  if (process.argv.includes('--finalize')) {
    const contentLabels = Object.fromEntries(locales.map(locale => [locale, Object.fromEntries(Object.keys(labels).map(key => [key, spec['pm.contentLabel.' + key][locale]]))]));
    const model = JSON.parse(fs.readFileSync(path.join(reportDirectory, 'last-working-model.json'), 'utf8'));
    const provenance = JSON.parse(fs.readFileSync(path.join(reportDirectory, 'provider-provenance.json'), 'utf8'));
    asset.description = 'Publication metadata base dictionaries and complete machine-generated content labels. No human linguistic review is recorded.';
    asset.contentLabels = contentLabels;
    asset.translationStatus = {provider: 'configured Antigravity relay', models: provenance.models, latestValidatedModel: model.model, generatedAt: new Date().toISOString(), locales: locales.length, labelsPerLocale: Object.keys(labels).length, sourceSpec: 'tools/i18n-batch3-publication-metadata.json', sourceSpecSha256: hash(specFile), placeholdersVerified: true, humanLinguisticReviewRecorded: false};
    fs.writeFileSync(assetFile, JSON.stringify(asset, null, 2) + '\n');
  }
  console.log('Verified ' + Object.keys(spec).length + ' keys × ' + locales.length + ' locales; placeholders match. Human linguistic review remains pending.');
}
