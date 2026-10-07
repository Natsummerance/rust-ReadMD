export interface FencedCode { language: string; code: string; from: number; to: number }

/** CommonMark fences: up to three spaces, matching marker and closing length. */
export function fencedCodeAt(text: string, cursor: number): FencedCode | undefined {
  let opened: { marker: string; language: string; from: number; content: number } | undefined;
  let offset = 0;
  for (const lineWithEnd of text.match(/[^\n]*(?:\n|$)/g) || []) {
    const line = lineWithEnd.replace(/\r?\n$/, '');
    if (!opened) {
      const match = /^ {0,3}(`{3,}|~{3,})([^\r\n]*)$/.exec(line);
      if (match && !(match[1][0] === '`' && match[2].includes('`'))) {
        opened = { marker: match[1], language: match[2].trim().split(/[\s{]/)[0].toLowerCase() || 'python', from: offset, content: offset + lineWithEnd.length };
      }
    } else {
      const closing = /^ {0,3}(`{3,}|~{3,})[ \t]*$/.exec(line);
      if (closing && closing[1][0] === opened.marker[0] && closing[1].length >= opened.marker.length) {
        const to = offset + lineWithEnd.length;
        if (cursor >= opened.from && cursor < to) return { language: opened.language, code: text.slice(opened.content, offset), from: opened.from, to };
        opened = undefined;
      }
    }
    offset += lineWithEnd.length;
  }
  return undefined;
}
