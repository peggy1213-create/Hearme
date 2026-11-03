import { afterEach, describe, expect, it } from 'vitest';
import { FakeClient, startGateway, type TestGateway } from './testutil/gateway.js';

/** A short meeting: connect, stream 6 meeting frames, stop, wait for `ended`. */
async function runMeeting(url: string, id: string): Promise<number> {
  const c = await FakeClient.connect(url);
  c.start(id);
  await c.waitUntil(() => c.acks.length >= 2, 10_000);
  for (let seq = 0; seq < 6; seq++) c.audio('meeting', seq);
  c.stop(id);
  await c.waitUntil(() => c.ended !== undefined, 10_000);
  const count = c.ended!.segmentCount;
  c.close();
  return count;
}

describe('load', () => {
  let tg: TestGateway;
  afterEach(async () => {
    await tg.close();
  });

  it('handles 50 simultaneous meetings with the mock vendor', async () => {
    tg = await startGateway();
    const counts = await Promise.all(
      Array.from({ length: 50 }, (_, i) => runMeeting(tg.liveUrl, `load-${i}`)),
    );
    expect(counts).toHaveLength(50);
    expect(counts.every((n) => n === 3)).toBe(true); // 6 frames / 2 per segment
  }, 30_000);
});
