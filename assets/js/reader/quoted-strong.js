'use strict';

// A matched typographic quote can be wrapped in ** without spaces in CJK
// prose. Use Marked's inline mask so escaped characters, links, and code
// cannot supply the closing quote. The token's raw slice stays byte-for-byte.
(function (markdown) {
  if (!markdown?.use || !markdown.Tokenizer) return;
  const quotes = new Map([['“','”'], ['‘','’'], ['「','」'], ['『','』'], ['《','》'], ['〈','〉'], ['【','】'], ['（','）']]);
  const eastAsian = /[\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}]/u;
  markdown.use({ tokenizer: {
    emStrong(src, mask, previous) {
      const open = src.startsWith('**') && src[2];
      const close = quotes.get(open);
      if (!close) return false;
      const visible = mask.slice(-src.length);
      if (!visible.startsWith('**' + open)) return false;
      let depth = 1;
      for (let i = 3; i < Math.min(visible.length, 4096); i++) {
        if (visible[i] === '\n' || visible[i] === '\r') break;
        if (visible[i] === open) depth++;
        if (visible[i] !== close || --depth !== 0) continue;
        const end = i + 3;
        if (visible.slice(i + 1, end) !== '**' || visible[end] === '*' ||
            !eastAsian.test(previous || '') && !eastAsian.test(String.fromCodePoint(src.codePointAt(end) ?? 0))) break;
        const text = src.slice(2, i + 1);
        return { type: 'strong', raw: src.slice(0, end), text, tokens: this.lexer.inlineTokens(text) };
      }
      return false;
    },
  } });
})(globalThis.marked);
