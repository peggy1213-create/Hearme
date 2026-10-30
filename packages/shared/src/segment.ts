import { z } from 'zod';

/** Which audio track a segment came from. */
export const TrackSchema = z.enum(['mic', 'meeting']);
export type Track = z.infer<typeof TrackSchema>;

/**
 * Language of a segment. `mixed` means English and Taiwanese Mandarin inside the
 * same segment (code-switching). Chinese is always Traditional (Taiwan); see
 * `packages/zhtw`.
 */
export const LangSchema = z.enum(['en', 'zh-TW', 'mixed']);
export type Lang = z.infer<typeof LangSchema>;

/** Which transcription pass produced the segment. */
export const PassSchema = z.enum(['live', 'final']);
export type Pass = z.infer<typeof PassSchema>;

/**
 * A single line of transcript.
 *
 * `speaker` is a stable key: `me` for the user's own microphone, or `S1`, `S2`…
 * for diarized meeting speakers. `speakerName` is the human label the user may set
 * (stored in edits.json); it is optional and purely cosmetic.
 */
export const TranscriptSegmentSchema = z.object({
  id: z.string(),
  meetingId: z.string(),
  track: TrackSchema,
  /** `me`, or `S1`, `S2`, … */
  speaker: z.string(),
  speakerName: z.string().optional(),
  startMs: z.number().int().nonnegative(),
  endMs: z.number().int().nonnegative(),
  text: z.string(),
  lang: LangSchema,
  isFinal: z.boolean(),
  pass: PassSchema,
  confidence: z.number().min(0).max(1).optional(),
});
export type TranscriptSegment = z.infer<typeof TranscriptSegmentSchema>;
