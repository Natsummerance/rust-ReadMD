import { createHash } from 'crypto';

const START = '<!-- readmd:openapi:start ';
const MAX_ENCODED = 1024 * 1024;
const MAX_DOCUMENT = 32 * 1024 * 1024;
const HEX = /^[a-f0-9]{64}$/;
const hash = (text: string) => createHash('sha256').update(text).digest('hex');
const lf = (text: string) => text.replace(/\r\n/g, '\n').replace(/\r/g, '\n');

export interface MetadataSpan {
  readonly from: number;
  readonly to: number;
  readonly operations: number;
  readonly references: number;
  readonly state?: 'previous' | 'updated';
}
export interface OpenAPIReviewProjection {
  readonly before?: MetadataSpan;
  readonly after: MetadataSpan;
}
interface Carrier {
  id: string;
  length: number;
  operations: number;
  references: number;
}
function object(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
function keys(value: Record<string, unknown>, required: string[], optional: string[] = []): boolean {
  return required.every(key => Object.hasOwn(value, key)) &&
    Object.keys(value).every(key => required.includes(key) || optional.includes(key));
}
function boundary(text: string, at: number): boolean {
  if (!Number.isSafeInteger(at) || at < 0 || at > text.length) return false;
  const left = text.charCodeAt(at - 1), right = text.charCodeAt(at);
  return !(left >= 0xd800 && left <= 0xdbff && right >= 0xdc00 && right <= 0xdfff);
}

/** A carrier is parsed only at a backend-owned edit boundary, never searched. */
function carrier(segment: string): Carrier | undefined {
  if (!segment.startsWith(START)) return;
  const newline = segment.indexOf('\n');
  if (newline < 0) return;
  const lineEnd = newline > 0 && segment[newline - 1] === '\r' ? newline - 1 : newline;
  const line = segment.slice(0, lineEnd);
  if (!line.endsWith(' -->')) return;
  const encoded = line.slice(START.length, -4);
  if (!encoded.length || encoded.length > MAX_ENCODED || !/^[A-Za-z0-9_-]+$/.test(encoded)) return;
  const bytes = Buffer.from(encoded, 'base64url');
  if (bytes.toString('base64url') !== encoded) return;
  const decoded = bytes.toString('utf8');
  if (!Buffer.from(decoded, 'utf8').equals(bytes)) return;
  let value: unknown;
  try { value = JSON.parse(decoded); } catch { return; }
  // The current producer emits compact JSON. Refuse ambiguous duplicate keys
  // or unfamiliar representations instead of hiding anything not understood.
  if (JSON.stringify(value) !== decoded || !object(value) ||
      !keys(value, ['schema', 'id', 'source', 'source_sha256', 'body_sha256', 'operations'], ['references']) ||
      value.schema !== 1 || typeof value.id !== 'string' || !HEX.test(value.id) ||
      typeof value.source !== 'string' || !value.source.length || Buffer.byteLength(value.source) > 256 ||
      typeof value.source_sha256 !== 'string' || !HEX.test(value.source_sha256) ||
      typeof value.body_sha256 !== 'string' || !HEX.test(value.body_sha256) || !object(value.operations) ||
      !Object.values(value.operations).every(digest => typeof digest === 'string' && HEX.test(digest))) return;
  const references = value.references ?? [];
  if (!Array.isArray(references) || !references.every(ref => object(ref) && keys(ref, ['source', 'revision']) &&
      typeof ref.source === 'string' && ref.source.length > 0 && typeof ref.revision === 'string' && HEX.test(ref.revision))) return;
  const end = '<!-- readmd:openapi:end ' + value.id + ' -->';
  if (!segment.endsWith(end)) return;
  const bodyEnd = segment.length - end.length;
  if (bodyEnd < newline + 1 || segment[bodyEnd - 1] !== '\n' ||
      hash(lf(segment.slice(newline + 1, bodyEnd))) !== value.body_sha256) return;
  return { id: value.id, length: lineEnd, operations: Object.keys(value.operations).length, references: references.length };
}

/** Unknown shapes keep the complete raw diff. This never changes a transaction. */
export function openAPIReviewProjection(before: string, after: string, transaction: unknown): OpenAPIReviewProjection | undefined {
  if (Buffer.byteLength(before) > MAX_DOCUMENT || Buffer.byteLength(after) > MAX_DOCUMENT || !object(transaction) ||
      !keys(transaction, ['base_revision', 'origin', 'edits']) || transaction.origin !== 'openapi' ||
      transaction.base_revision !== hash(before) || !Array.isArray(transaction.edits) || transaction.edits.length !== 1) return;
  const edit: unknown = transaction.edits[0];
  if (!object(edit) || !keys(edit, ['from', 'to', 'insert']) || typeof edit.from !== 'number' || typeof edit.to !== 'number' ||
      typeof edit.insert !== 'string' || !boundary(before, edit.from) || !boundary(before, edit.to) || edit.to < edit.from ||
      before.slice(0, edit.from) + edit.insert + before.slice(edit.to) !== after) return;
  const append = edit.from === before.length && edit.to === edit.from;
  if (!append && (edit.from === edit.to || (edit.from > 0 && before[edit.from - 1] !== '\n'))) return;
  const crlf = before.includes('\r\n') && !before.replace(/\r\n/g, '').includes('\n');
  const prefix = append && before.length ? (crlf ? '\r\n\r\n' : '\n\n') : '';
  if (!edit.insert.startsWith(prefix)) return;
  const current = carrier(edit.insert.slice(prefix.length));
  const previous = append ? undefined : carrier(before.slice(edit.from, edit.to));
  if (!current || (!append && (!previous || previous.id !== current.id))) return;
  const metadataChanged = previous && before.slice(edit.from, edit.from + previous.length) !== edit.insert.slice(prefix.length, prefix.length + current.length);
  const span = (at: number, value: Carrier, state?: 'previous' | 'updated'): MetadataSpan => Object.freeze({
    from: at, to: at + value.length, operations: value.operations, references: value.references, state,
  });
  return Object.freeze({ before: previous ? span(edit.from, previous, metadataChanged ? 'previous' : undefined) : undefined,
    after: span(edit.from + prefix.length, current, metadataChanged ? 'updated' : undefined) });
}

/** Only native read-only view text passes through this function. */
export function metadataViewText(raw: string, span: MetadataSpan | undefined, summary: string): string {
  if (!span || !summary || summary.length > 512 || /[\r\n]/.test(summary) || summary.includes('-->')) return raw;
  return raw.slice(0, span.from) + '<!-- ' + summary + ' -->' + raw.slice(span.to);
}
