import { containsSimplified, toTaiwanTraditional } from '@hearme/zhtw';
import type { TranscriptSegment } from '@hearme/shared';

/**
 * Last line of defence before a segment leaves the gateway: every segment's text
 * is run through `toTaiwanTraditional`, so no Simplified character can ever reach a
 * client even if an adapter missed it. The STT adapters already normalise, so in
 * practice this is a no-op — but CLAUDE.md requires that we never display
 * Simplified, and the privacy/Traditional test depends on this guard.
 */
export function ensureTraditional(segment: TranscriptSegment): TranscriptSegment {
  if (!containsSimplified(segment.text)) return segment;
  return { ...segment, text: toTaiwanTraditional(segment.text) };
}

/** True if any segment still carries Simplified characters (used by tests). */
export function hasSimplified(segments: TranscriptSegment[]): boolean {
  return segments.some((s) => containsSimplified(s.text));
}
