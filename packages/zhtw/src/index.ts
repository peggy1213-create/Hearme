import * as OpenCC from 'opencc-js';

/**
 * Traditional Chinese (Taiwan) normalisation.
 *
 * All Chinese text in Hearme must be Traditional characters with Taiwan wording
 * before it is shown or saved — never Simplified. This wraps OpenCC's `s2twp`
 * profile (Simplified → Traditional, Taiwan standard, with Taiwanese phrase
 * substitution: 软件 → 軟體, 视频 → 影片). See CLAUDE.md.
 *
 * In opencc-js terms `from: 'cn'` (Simplified) to `to: 'twp'` (Taiwan + phrases) is
 * the `s2twp` conversion.
 */
const converter = OpenCC.Converter({ from: 'cn', to: 'twp' });

/**
 * Convert any Chinese text to Traditional (Taiwan). Text already Traditional, or
 * with no Chinese at all (English, punctuation, numbers), is returned effectively
 * unchanged, so this is safe to run on mixed English/Mandarin segments.
 */
export function toTaiwan(text: string): string {
  if (!text) return text;
  return converter(text);
}
