import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { startGateway, type TestGateway } from './testutil/gateway.js';

describe('gateway health', () => {
  let tg: TestGateway;

  beforeEach(async () => {
    tg = await startGateway();
  });

  afterEach(async () => {
    await tg.close();
  });

  it('serves health', async () => {
    const res = await tg.gateway.app.inject({ method: 'GET', url: '/health' });
    expect(res.statusCode).toBe(200);
    const body = res.json();
    expect(body.status).toBe('ok');
    expect(body.service).toBe('hearme-gateway');
    expect(body.protocolVersion).toBe(2);
  });
});
