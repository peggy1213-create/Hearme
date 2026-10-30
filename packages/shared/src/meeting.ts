import { z } from 'zod';

/** Meeting platform the audio came from. */
export const PlatformSchema = z.enum(['zoom', 'meet', 'teams', 'webex', 'other']);
export type Platform = z.infer<typeof PlatformSchema>;

/** Lifecycle of a meeting in the local library. */
export const MeetingStatusSchema = z.enum([
  'recording', // actively capturing audio
  'stopped', // capture ended, nothing processed yet
  'transcribing', // final batch pass running
  'summarizing', // summary + action items being generated
  'done', // transcript and summary ready
  'error', // capture or processing failed
]);
export type MeetingStatus = z.infer<typeof MeetingStatusSchema>;

/** State of opt-in Google Drive sync for a meeting folder. */
export const DriveSyncStateSchema = z.enum(['off', 'pending', 'synced', 'error']);
export type DriveSyncState = z.infer<typeof DriveSyncStateSchema>;

export const DriveSyncSchema = z.object({
  state: DriveSyncStateSchema,
  driveFolderId: z.string().optional(),
  lastSyncedAt: z.string().datetime().optional(),
});
export type DriveSync = z.infer<typeof DriveSyncSchema>;

/**
 * One meeting. `folderPath` is the local folder that is the single source of
 * truth (see MEETING_FOLDER.md). Timestamps are ISO-8601 strings.
 */
export const MeetingSchema = z.object({
  id: z.string(),
  title: z.string(),
  platform: PlatformSchema,
  startedAt: z.string().datetime(),
  endedAt: z.string().datetime().optional(),
  folderPath: z.string(),
  status: MeetingStatusSchema,
  driveSync: DriveSyncSchema.optional(),
});
export type Meeting = z.infer<typeof MeetingSchema>;
