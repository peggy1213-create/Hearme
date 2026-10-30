import { z } from 'zod';
import { MeetingSchema } from './meeting.js';
import { LangSchema, TranscriptSegmentSchema } from './segment.js';

/**
 * Schemas for the JSON files inside a local meeting folder. Every file carries a
 * `version`; bump it and update MEETING_FOLDER.md when a shape changes.
 */
export const MEETING_FOLDER_VERSION = 1 as const;

/** Standard file names inside a meeting folder. */
export const MeetingFolderFiles = {
  micAudio: 'mic.ogg',
  meetingAudio: 'meeting.ogg',
  mixedAudio: 'mixed.ogg',
  meeting: 'meeting.json',
  transcriptJson: 'transcript.json',
  transcriptMd: 'transcript.md',
  summaryJson: 'summary.json',
  summaryMd: 'summary.md',
  edits: 'edits.json',
} as const;

/** meeting.json — metadata plus the live-pass segments captured during the call. */
export const MeetingFileSchema = z.object({
  version: z.literal(MEETING_FOLDER_VERSION),
  meeting: MeetingSchema,
  liveSegments: z.array(TranscriptSegmentSchema),
});
export type MeetingFile = z.infer<typeof MeetingFileSchema>;

/** transcript.json — the cleaned final-pass transcript. */
export const TranscriptFileSchema = z.object({
  version: z.literal(MEETING_FOLDER_VERSION),
  meetingId: z.string(),
  generatedAt: z.string().datetime(),
  segments: z.array(TranscriptSegmentSchema),
});
export type TranscriptFile = z.infer<typeof TranscriptFileSchema>;

export const ActionItemSchema = z.object({
  id: z.string(),
  text: z.string(),
  owner: z.string().optional(),
  done: z.boolean().default(false),
});
export type ActionItem = z.infer<typeof ActionItemSchema>;

/** summary.json — summary plus action items, in the chosen output language. */
export const SummaryFileSchema = z.object({
  version: z.literal(MEETING_FOLDER_VERSION),
  meetingId: z.string(),
  generatedAt: z.string().datetime(),
  language: LangSchema,
  summary: z.string(),
  actionItems: z.array(ActionItemSchema),
});
export type SummaryFile = z.infer<typeof SummaryFileSchema>;

/** One user correction to a segment's text. */
export const SegmentEditSchema = z.object({
  segmentId: z.string(),
  text: z.string(),
});
export type SegmentEdit = z.infer<typeof SegmentEditSchema>;

/**
 * edits.json — the user's corrections. `speakerNames` maps a speaker key
 * (`me`, `S1`, …) to a display name. Edits are kept separate so the generated
 * transcript and summary can be regenerated without losing them.
 */
export const EditsFileSchema = z.object({
  version: z.literal(MEETING_FOLDER_VERSION),
  meetingId: z.string(),
  updatedAt: z.string().datetime(),
  speakerNames: z.record(z.string(), z.string()),
  segmentEdits: z.array(SegmentEditSchema),
});
export type EditsFile = z.infer<typeof EditsFileSchema>;
