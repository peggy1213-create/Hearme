import { z } from 'zod';
import { PlatformSchema } from './meeting.js';
import { LangSchema, TrackSchema, TranscriptSegmentSchema } from './segment.js';

/**
 * Hearme gateway WebSocket protocol.
 *
 * The desktop app opens one WebSocket per meeting. It sends `start`, then a stream
 * of `audio` frames per track, then `stop`. The gateway relays audio to the STT
 * vendor and streams back `segment` messages, an optional `error`, and a final
 * `ended`. The gateway stores nothing; see PROTOCOL.md.
 */

export const PROTOCOL_VERSION = 2 as const;

/**
 * Audio codec of the `audio` frames.
 * - `opus`: Opus-encoded (the production path); the gateway decodes to PCM before
 *   relaying to the STT vendor.
 * - `pcm`: raw little-endian 16-bit mono PCM at `sampleRate`, relayed as-is. Used
 *   by the replay tool and tests so they need no Opus encoder.
 */
export const CodecSchema = z.enum(['opus', 'pcm']);
export type Codec = z.infer<typeof CodecSchema>;

// --- client -> server ---

/** Opens a session. Sent once, first. On reconnect, sent again with the same `meetingId`. */
export const ClientStartSchema = z.object({
  type: z.literal('start'),
  protocolVersion: z.literal(PROTOCOL_VERSION),
  meetingId: z.string(),
  platform: PlatformSchema,
  /** Languages to bias the STT model toward. */
  languages: z.array(LangSchema).default(['en', 'zh-TW']),
  /** PCM sample rate of the decoded audio the gateway should expect, in Hz. */
  sampleRate: z.number().int().positive(),
  /** Audio codec of the frames that follow. */
  codec: CodecSchema.default('opus'),
  /** Custom vocabulary (names, products, jargon) passed to the vendor when supported. */
  vocabulary: z.array(z.string()).optional(),
});
export type ClientStart = z.infer<typeof ClientStartSchema>;

/**
 * One chunk of audio for a track. `data` is base64-encoded Opus. (Implementations
 * may also send raw binary WebSocket frames tagged out of band; the JSON form here
 * is the canonical description.)
 */
export const ClientAudioSchema = z.object({
  type: z.literal('audio'),
  track: TrackSchema,
  /** Monotonic sequence number per track, starting at 0. */
  seq: z.number().int().nonnegative(),
  /** Milliseconds from the start of the meeting for the first sample in this chunk. */
  timestampMs: z.number().int().nonnegative(),
  data: z.string(), // base64 Opus
});
export type ClientAudio = z.infer<typeof ClientAudioSchema>;

/** Ends the session. The gateway flushes STT and replies with `ended`. */
export const ClientStopSchema = z.object({
  type: z.literal('stop'),
  meetingId: z.string(),
});
export type ClientStop = z.infer<typeof ClientStopSchema>;

export const ClientMessageSchema = z.discriminatedUnion('type', [
  ClientStartSchema,
  ClientAudioSchema,
  ClientStopSchema,
]);
export type ClientMessage = z.infer<typeof ClientMessageSchema>;

// --- server -> client ---

/** A transcript segment from the live streaming pass. */
export const ServerSegmentSchema = z.object({
  type: z.literal('segment'),
  segment: TranscriptSegmentSchema,
});
export type ServerSegment = z.infer<typeof ServerSegmentSchema>;

/**
 * Acknowledges how far the gateway has consumed a track. Sent right after `start`
 * (including on reconnect) and as frames are consumed, so a client that reconnects
 * knows to resend from `lastSeq + 1`. `lastSeq` is `-1` when no frame has been
 * received for the track yet.
 */
export const ServerAckSchema = z.object({
  type: z.literal('ack'),
  track: TrackSchema,
  lastSeq: z.number().int().min(-1),
});
export type ServerAck = z.infer<typeof ServerAckSchema>;

/** Error codes the gateway may return. */
export const ServerErrorCodeSchema = z.enum([
  'bad_request',
  'unauthorized',
  'quota_exceeded',
  'stt_unavailable',
  'internal',
]);
export type ServerErrorCode = z.infer<typeof ServerErrorCodeSchema>;

export const ServerErrorSchema = z.object({
  type: z.literal('error'),
  code: ServerErrorCodeSchema,
  /** Human-readable; MUST NOT contain transcript or audio content. */
  message: z.string(),
  /** If true the session is over and the socket will close. */
  fatal: z.boolean().default(false),
});
export type ServerError = z.infer<typeof ServerErrorSchema>;

/** Final message of a session. */
export const ServerEndedSchema = z.object({
  type: z.literal('ended'),
  meetingId: z.string(),
  /** Total live segments emitted during the session. */
  segmentCount: z.number().int().nonnegative(),
});
export type ServerEnded = z.infer<typeof ServerEndedSchema>;

export const ServerMessageSchema = z.discriminatedUnion('type', [
  ServerSegmentSchema,
  ServerAckSchema,
  ServerErrorSchema,
  ServerEndedSchema,
]);
export type ServerMessage = z.infer<typeof ServerMessageSchema>;
