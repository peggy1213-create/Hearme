import * as OpenCC from 'opencc-js';

/**
 * Traditional Chinese (Taiwan) normalisation and scoring helpers.
 *
 * All Chinese text in Hearme must be Traditional characters with Taiwan wording
 * before it is shown or saved — never Simplified. See CLAUDE.md.
 *
 * In opencc-js terms `from: 'cn'` (Simplified) to `to: 'twp'` (Taiwan + phrases)
 * is the OpenCC `s2twp` conversion: Simplified → Traditional, Taiwan standard,
 * with Taiwanese phrase substitution (软件 → 軟體, 视频 → 影片).
 */
const s2twp = OpenCC.Converter({ from: 'cn', to: 'twp' });

/**
 * Character-level Simplified → Traditional (OpenCC standard), used only to decide
 * whether a character is Simplified. We avoid the phrase profile here so that an
 * already-Traditional phrase is never flagged.
 */
const s2tChars = OpenCC.Converter({ from: 'cn', to: 't' });

const HAN = /\p{Script=Han}/u;

/**
 * Convert any Chinese text to Traditional (Taiwan), swapping mainland terms for
 * Taiwan ones (s2twp). Text already Traditional, or with no Chinese at all
 * (English, punctuation, numbers), is returned effectively unchanged, so this is
 * safe to run on mixed English/Mandarin segments.
 */
export function toTaiwanTraditional(text: string): string {
  if (!text) return text;
  return s2twp(text);
}

/**
 * @deprecated Use {@link toTaiwanTraditional}. Kept as an alias so existing
 * callers keep working.
 */
export const toTaiwan = toTaiwanTraditional;

/**
 * Normalise text for error-rate scoring, so that cosmetic differences are not
 * counted as errors:
 * - full-width Latin letters, digits and the full-width space → half-width;
 * - punctuation and symbols removed (letters, digits and Han kept);
 * - English lowercased (Han is unaffected);
 * - runs of whitespace collapsed to a single space, ends trimmed.
 *
 * Chinese is NOT converted here — do that with {@link toTaiwanTraditional} first
 * if you want Simplified/Traditional treated as equal.
 */
export function normaliseForScoring(text: string): string {
  if (!text) return '';
  // Full-width ASCII (U+FF01-U+FF5E) -> half-width (U+0021-U+007E).
  let s = text.replace(/[！-～]/g, (c) => String.fromCharCode(c.charCodeAt(0) - 0xfee0));
  // Drop anything that is not a letter, number or whitespace (punctuation, symbols).
  // The ideographic space (U+3000) is part of \s, so it is kept here and then
  // collapsed to a single ASCII space by the final step.
  s = s.replace(/[^\p{L}\p{N}\s]/gu, ' ');
  s = s.toLowerCase();
  return s.replace(/\s+/gu, ' ').trim();
}

/**
 * Tokenise mixed English/Mandarin text for scoring code-switched speech:
 * each Han character becomes its own token, each run of Latin letters/digits
 * becomes a word token. Input is normalised with {@link normaliseForScoring}
 * first.
 *
 * e.g. "這個 sprint 我們先 focus 在 Core Audio" →
 *      ['這','個','sprint','我','們','先','focus','在','core','audio']
 */
export function tokenizeMixed(text: string): string[] {
  const tokens: string[] = [];
  let word = '';
  const flush = (): void => {
    if (word) {
      tokens.push(word);
      word = '';
    }
  };
  for (const ch of normaliseForScoring(text)) {
    if (ch === ' ') {
      flush();
    } else if (HAN.test(ch)) {
      flush();
      tokens.push(ch);
    } else {
      word += ch;
    }
  }
  flush();
  return tokens;
}

/**
 * Whether the text contains any Simplified Chinese character. Used by the STT
 * benchmark to flag vendor output that returned Simplified before we converted it
 * (CLAUDE.md: we must never display Simplified). A character counts as Simplified
 * when converting it to Traditional (OpenCC standard, character level) changes it.
 */
export function containsSimplified(text: string): boolean {
  for (const ch of text) {
    if (HAN.test(ch) && s2tChars(ch) !== ch) return true;
  }
  return false;
}
