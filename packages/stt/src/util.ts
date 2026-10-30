import { toTaiwanTraditional } from '@hearme/zhtw';
import type { Lang } from '@hearme/shared';

/**
 * Shared helpers for the STT adapters. Keeping language detection, Chinese
 * normalisation and speaker-key mapping in one place means every vendor emits
 * segments the same way — and every Chinese string goes through
 * `toTaiwanTraditional` before it leaves the adapter (CLAUDE.md: never emit
 * Simplified).
 */

const HAN = /\p{Script=Han}/u;
const LATIN = /[A-Za-z]/u;

/** Guess a segment's language from its text (en / zh-TW / mixed). */
export function detectLang(text: string): Lang {
  const han = HAN.test(text);
  const latin = LATIN.test(text);
  if (han && latin) return 'mixed';
  if (han) return 'zh-TW';
  return 'en';
}

/**
 * Normalise any Chinese to Traditional (Taiwan) — the one exit point for text.
 * When `enabled` is `false` the text is returned untouched, so a caller (the
 * benchmark) can inspect the vendor's raw output before converting.
 */
export function normaliseText(text: string, enabled = true): string {
  return enabled ? toTaiwanTraditional(text) : text;
}

/**
 * Map our `Lang[]` to the two-letter language hints the vendors expect
 * (`zh-TW`/`mixed` → `zh`, plus `en`), de-duplicated and defaulting to
 * English + Mandarin so code-switching is always covered.
 */
export function languageHints(langs: Lang[]): string[] {
  const hints = new Set<string>();
  for (const l of langs) {
    if (l === 'en') hints.add('en');
    else hints.add('zh');
  }
  if (hints.size === 0) {
    hints.add('en');
    hints.add('zh');
  }
  return [...hints];
}

/** Turn a vendor speaker id (string or number) into a stable `S<n>` key. */
export function speakerKey(raw: string | number | undefined): string {
  if (raw === undefined || raw === null || raw === '') return 'S1';
  return `S${raw}`;
}

let segCounter = 0;
/** Monotonic segment id, scoped by meeting. */
export function nextSegmentId(meetingId: string): string {
  segCounter += 1;
  return `${meetingId}-seg-${segCounter}`;
}
