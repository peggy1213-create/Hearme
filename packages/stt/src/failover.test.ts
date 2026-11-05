import { describe, expect, it } from 'vitest';
import type { TranscriptSegment } from '@hearme/shared';
import { FailoverAdapter } from './failover.js';
import { MockAdapter } from './vendors/mock.js';

const streamOpts = {
  meetingId: 'm-1',
  platform: 'zoom' as const,
  languages: ['en' as const, 'zh-TW' as const],
  sampleRate: 16000,
};

describe('MockAdapter', () => {
  it('emits Traditional segments from Simplified source when normalising', async () => {
    const got: TranscriptSegment[] = [];
    const mock = new MockAdapter({
      script: ['这个 sprint 我们先 focus'],
      framesPerSegment: 1,
    });
    const stream = await mock.openStream(streamOpts, {
      onSegment: (s) => got.push(s),
      onError: () => {},
    });
    stream.pushAudio('meeting', new Uint8Array(4), 0);
    await stream.close();
    expect(got).toHaveLength(1);
    expect(got[0]!.text).toContain('這個'); // Simplified 这个 -> Traditional 這個
    expect(got[0]!.text).not.toContain('这');
  });

  it('calls onError after failAfterFrames', async () => {
    let err: Error | undefined;
    const mock = new MockAdapter({ failAfterFrames: 2 });
    const stream = await mock.openStream(streamOpts, {
      onSegment: () => {},
      onError: (e) => (err = e),
    });
    stream.pushAudio('meeting', new Uint8Array(4), 0);
    stream.pushAudio('meeting', new Uint8Array(4), 20);
    expect(err?.message).toMatch(/simulated vendor failure/);
  });
});

describe('FailoverAdapter', () => {
  it('switches to the fallback mid-stream and keeps emitting', async () => {
    const got: TranscriptSegment[] = [];
    let failedOver = false;
    const primary = new MockAdapter({ failAfterFrames: 2, framesPerSegment: 1 });
    const fallback = new MockAdapter({ script: ['fallback line'], framesPerSegment: 1 });
    const adapter = new FailoverAdapter(primary, fallback, () => (failedOver = true));

    let errored = false;
    const stream = await adapter.openStream(streamOpts, {
      onSegment: (s) => got.push(s),
      onError: () => (errored = true),
    });
    // Frame 1: primary emits. Frame 2: primary fails -> switch. Later frames: fallback.
    stream.pushAudio('meeting', new Uint8Array(4), 0);
    stream.pushAudio('meeting', new Uint8Array(4), 20);
    await new Promise((r) => setTimeout(r, 0)); // let the async switch settle
    stream.pushAudio('meeting', new Uint8Array(4), 40);
    await new Promise((r) => setTimeout(r, 0));
    await stream.close();

    expect(failedOver).toBe(true);
    expect(errored).toBe(false); // the primary's failure must not surface
    expect(got.some((s) => s.text === 'fallback line')).toBe(true);
  });
});
