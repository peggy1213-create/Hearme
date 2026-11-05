import { createHash } from 'node:crypto';
import { afterEach, describe, expect, it } from 'vitest';
import { resolveUserId } from './auth/authenticate.js';
import { generatePkce } from './auth/oauth.js';
import {
  issueTokens,
  tokenConfigFromEnv,
  verifyAccessToken,
  verifyRefreshToken,
} from './auth/tokens.js';
import { upsertUser } from './db/store.js';
import { startGateway, type TestGateway } from './testutil/gateway.js';

const cfg = tokenConfigFromEnv({ JWT_SECRET: 'test-secret' } as NodeJS.ProcessEnv);

describe('tokens', () => {
  it('issues and verifies an access/refresh pair', async () => {
    const { accessToken, refreshToken } = await issueTokens(cfg, 'user-1');
    expect(await verifyAccessToken(cfg, accessToken)).toBe('user-1');
    expect(await verifyRefreshToken(cfg, refreshToken)).toBe('user-1');
  });

  it('does not accept a refresh token as an access token (or vice versa)', async () => {
    const { accessToken, refreshToken } = await issueTokens(cfg, 'user-2');
    await expect(verifyAccessToken(cfg, refreshToken)).rejects.toBeInstanceOf(Error);
    await expect(verifyRefreshToken(cfg, accessToken)).rejects.toBeInstanceOf(Error);
  });

  it('rejects a token signed with a different secret', async () => {
    const other = tokenConfigFromEnv({ JWT_SECRET: 'other-secret' } as NodeJS.ProcessEnv);
    const { accessToken } = await issueTokens(other, 'user-3');
    await expect(verifyAccessToken(cfg, accessToken)).rejects.toBeInstanceOf(Error);
  });
});

describe('PKCE', () => {
  it('produces an S256 challenge of the verifier', () => {
    const { verifier, challenge } = generatePkce();
    const expected = createHash('sha256')
      .update(verifier)
      .digest('base64')
      .replace(/\+/g, '-')
      .replace(/\//g, '_')
      .replace(/=+$/, '');
    expect(challenge).toBe(expected);
  });
});

describe('auth routes and resolution', () => {
  let tg: TestGateway;
  afterEach(async () => {
    await tg.close();
  });

  it('rejects an unknown OAuth provider', async () => {
    tg = await startGateway();
    const res = await tg.gateway.app.inject({
      method: 'GET',
      url: '/v1/auth/facebook/url?redirectUri=http://127.0.0.1:9000/cb',
    });
    expect(res.statusCode).toBe(400);
  });

  it('refreshes tokens, and rejects a bad refresh token', async () => {
    tg = await startGateway();
    const user = await upsertUser(tg.db.db, { provider: 'google', providerSub: 'sub-1' });
    const { refreshToken } = await issueTokens(tg.gateway.context.tokens, user.id);

    const ok = await tg.gateway.app.inject({
      method: 'POST',
      url: '/v1/auth/refresh',
      payload: { refreshToken },
    });
    expect(ok.statusCode).toBe(200);
    expect(ok.json().accessToken).toBeTruthy();

    const bad = await tg.gateway.app.inject({
      method: 'POST',
      url: '/v1/auth/refresh',
      payload: { refreshToken: 'nope' },
    });
    expect(bad.statusCode).toBe(401);
  });

  it('maps the dev bypass token to a stable dev user', async () => {
    tg = await startGateway();
    const id1 = await resolveUserId(tg.gateway.context, 'Bearer dev-bypass', undefined);
    const id2 = await resolveUserId(tg.gateway.context, undefined, 'dev-bypass');
    expect(id1).toBe(id2); // same (provider=dev, sub=dev-user) row
  });
});
