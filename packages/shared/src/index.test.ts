import { describe, expect, it } from 'vitest';
import {
  ClientMessageSchema,
  MeetingFileSchema,
  MeetingSchema,
  PROTOCOL_VERSION,
  ServerMessageSchema,
  TranscriptSegmentSchema,
  MEETING_FOLDER_VERSION,
} from './index.js';

describe('TranscriptSegment', () => {
  it('parses a valid live segment', () => {
    const seg = TranscriptSegmentSchema.parse({
      id: 'seg-1',
      meetingId: 'm-1',
      track: 'mic',
      speaker: 'me',
      startMs: 0,
      endMs: 1200,
      text: '大家好 let us begin',
      lang: 'mixed',
      isFinal: false,
      pass: 'live',
    });
    expect(seg.speaker).toBe('me');
    expect(seg.lang).toBe('mixed');
  });

  it('rejects an unknown track', () => {
    expect(() =>
      TranscriptSegmentSchema.parse({
        id: 'x',
        meetingId: 'm',
        track: 'speaker',
        speaker: 'S1',
        startMs: 0,
        endMs: 1,
        text: 'hi',
        lang: 'en',
        isFinal: true,
        pass: 'final',
      }),
    ).toThrow();
  });
});

describe('Meeting', () => {
  it('parses a meeting with optional drive sync', () => {
    const m = MeetingSchema.parse({
      id: 'm-1',
      title: 'Weekly sync',
      platform: 'zoom',
      startedAt: '2026-10-30T09:00:00.000Z',
      folderPath: 'C:/Users/me/Documents/Hearme Meetings/m-1',
      status: 'recording',
    });
    expect(m.driveSync).toBeUndefined();
  });
});

describe('gateway protocol', () => {
  it('parses a start message', () => {
    const msg = ClientMessageSchema.parse({
      type: 'start',
      protocolVersion: PROTOCOL_VERSION,
      meetingId: 'm-1',
      platform: 'meet',
      sampleRate: 48000,
    });
    expect(msg.type).toBe('start');
    if (msg.type === 'start') {
      expect(msg.languages).toEqual(['en', 'zh-TW']);
      expect(msg.codec).toBe('opus');
    }
  });

  it('parses a server ended message', () => {
    const msg = ServerMessageSchema.parse({
      type: 'ended',
      meetingId: 'm-1',
      segmentCount: 42,
    });
    expect(msg.type).toBe('ended');
  });
});

describe('meeting folder', () => {
  it('parses a meeting.json file', () => {
    const file = MeetingFileSchema.parse({
      version: MEETING_FOLDER_VERSION,
      meeting: {
        id: 'm-1',
        title: 'Weekly sync',
        platform: 'teams',
        startedAt: '2026-10-30T09:00:00.000Z',
        folderPath: '/tmp/m-1',
        status: 'done',
      },
      liveSegments: [],
    });
    expect(file.version).toBe(1);
  });
});
