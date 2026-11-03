import { afterEach, describe, expect, it } from 'vitest';
import { containsSimplified } from '@hearme/zhtw';
import { startGateway, type TestGateway } from './testutil/gateway.js';

describe('batch (final pass) endpoint', () => {
  let tg: TestGateway;
  afterEach(async () => {
    await tg.close();
  });

  it('returns final Traditional segments and rejects an unauthenticated call', async () => {
    tg = await startGateway();

    const ok = await tg.gateway.app.inject({
      method: 'POST',
      url: '/v1/batch?meetingId=m-final&languages=en,zh-TW',
      headers: { authorization: 'Bearer dev-bypass', 'content-type': 'application/octet-stream' },
      payload: Buffer.from('fake-audio-bytes'),
    });
    expect(ok.statusCode).toBe(200);
    const body = ok.json();
    expect(body.meetingId).toBe('m-final');
    expect(body.segments.length).toBeGreaterThan(0);
    expect(body.segments.every((s: { pass: string }) => s.pass === 'final')).toBe(true);
    expect(body.segments.every((s: { text: string }) => !containsSimplified(s.text))).toBe(true);

    const unauth = await tg.gateway.app.inject({
      method: 'POST',
      url: '/v1/batch?meetingId=m-final',
      headers: { 'content-type': 'application/octet-stream' },
      payload: Buffer.from('x'),
    });
    expect(unauth.statusCode).toBe(401);
  });
});
