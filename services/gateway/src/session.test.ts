import { afterEach, describe, expect, it } from 'vitest';
import { containsSimplified } from '@hearme/zhtw';
import { FailoverAdapter, MockAdapter } from '@hearme/stt';
import { FakeClient, startGateway, type TestGateway } from './testutil/gateway.js';

describe('live session', () => {
  let tg: TestGateway;
  afterEach(async () => {
    if (tg) await tg.close();
  });

  it('acks both tracks on start and streams live segments, then ends', async () => {
    tg = await startGateway();
    const c = await FakeClient.connect(tg.liveUrl);
    c.start('m-1');
    await c.waitUntil(() => c.acks.length >= 2);
    expect(c.acks.map((a) => a.track).sort()).toEqual(['meeting', 'mic']);
    expect(c.acks.every((a) => a.lastSeq === -1)).toBe(true);

    for (let seq = 0; seq < 6; seq++) c.audio('meeting', seq);
    await c.waitUntil(() => c.segments.length >= 3);
    c.stop('m-1');
    await c.waitUntil(() => c.ended !== undefined);
    expect(c.ended?.segmentCount).toBe(3);
    c.close();
  });

  it('labels the mic track as "me" and never emits Simplified Chinese', async () => {
    tg = await startGateway();
    const c = await FakeClient.connect(tg.liveUrl);
    c.start('m-2');
    await c.waitUntil(() => c.acks.length >= 2);

    for (let seq = 0; seq < 2; seq++) c.audio('mic', seq);
    await c.waitUntil(() => c.segments.length >= 1);
    expect(c.segments[0]!.track).toBe('mic');
    expect(c.segments[0]!.speaker).toBe('me');

    // Drive enough meeting frames to emit the Simplified-source script line.
    for (let seq = 0; seq < 4; seq++) c.audio('meeting', seq);
    await c.waitUntil(() => c.segments.some((s) => s.text.includes('這個')));
    expect(c.segments.every((s) => !containsSimplified(s.text))).toBe(true);
    c.close();
  });

  it('resumes after a dropped socket without losing or duplicating words', async () => {
    tg = await startGateway();

    // Baseline: 10 meeting frames, no drop -> 5 segments (framesPerSegment 2).
    const base = await FakeClient.connect(tg.liveUrl);
    base.start('m-base');
    await base.waitUntil(() => base.acks.length >= 2);
    for (let seq = 0; seq < 10; seq++) base.audio('meeting', seq);
    await base.waitUntil(() => base.segments.length >= 5);
    base.stop('m-base');
    await base.waitUntil(() => base.ended !== undefined);
    const baseline = base.ended!.segmentCount;
    base.close();
    expect(baseline).toBe(5);

    // Resume run: send 0..5, drop, reconnect, resend from 4 (4,5 are duplicates).
    const a = await FakeClient.connect(tg.liveUrl);
    a.start('m-resume');
    await a.waitUntil(() => a.acks.length >= 2);
    for (let seq = 0; seq <= 5; seq++) a.audio('meeting', seq);
    await a.waitUntil(() => a.segments.length >= 3);
    a.close(); // drop without stop

    const b = await FakeClient.connect(tg.liveUrl);
    b.start('m-resume');
    await b.waitUntil(() => b.acks.some((ack) => ack.track === 'meeting'));
    const resumeAck = b.acks.find((ack) => ack.track === 'meeting')!;
    expect(resumeAck.lastSeq).toBe(5); // gateway kept the cursor across the drop
    for (let seq = 4; seq < 10; seq++) b.audio('meeting', seq); // 4,5 replayed -> deduped
    b.stop('m-resume');
    await b.waitUntil(() => b.ended !== undefined);
    expect(b.ended!.segmentCount).toBe(baseline); // no words lost, none duplicated
    b.close();
  });

  it('fails over to the fallback vendor mid-meeting', async () => {
    tg = await startGateway({
      buildLiveAdapter: () =>
        new FailoverAdapter(
          new MockAdapter({ failAfterFrames: 3, framesPerSegment: 2 }),
          new MockAdapter({ script: ['fallback transcript line'], framesPerSegment: 2 }),
        ),
    });
    const c = await FakeClient.connect(tg.liveUrl);
    c.start('m-failover');
    await c.waitUntil(() => c.acks.length >= 2);
    for (let seq = 0; seq < 10; seq++) c.audio('meeting', seq);
    await c.waitUntil(() => c.segments.some((s) => s.text === 'fallback transcript line'));
    expect(c.errors.filter((e) => e.fatal)).toHaveLength(0); // the failover was transparent
    c.close();
  });

  it('rejects a start when the monthly quota is exhausted', async () => {
    tg = await startGateway({ config: { monthlyQuotaMinutes: 0 } });
    const c = await FakeClient.connect(tg.liveUrl);
    c.start('m-quota');
    await c.waitUntil(() => c.errors.length >= 1);
    expect(c.errors[0]!.code).toBe('quota_exceeded');
    expect(c.errors[0]!.fatal).toBe(true);
    c.close();
  });

  it('rejects an invalid token when not in development', async () => {
    tg = await startGateway({ config: { isDev: false, devBypassToken: undefined } });
    const c = await FakeClient.connect(tg.liveUrl, 'not-a-real-token');
    c.start('m-auth');
    await c.waitUntil(() => c.errors.length >= 1);
    expect(c.errors[0]!.code).toBe('unauthorized');
    c.close();
  });
});
