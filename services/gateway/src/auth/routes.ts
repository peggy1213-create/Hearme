import { randomBytes } from 'node:crypto';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import type { GatewayContext } from '../context.js';
import { upsertUser } from '../db/store.js';
import { buildAuthUrl, exchangeCode, generatePkce, providerConfig } from './oauth.js';
import { issueTokens, verifyRefreshToken } from './tokens.js';

const ProviderSchema = z.enum(['google', 'microsoft']);

const UrlQuerySchema = z.object({ redirectUri: z.string().url() });
const ExchangeBodySchema = z.object({
  code: z.string().min(1),
  codeVerifier: z.string().min(1),
  redirectUri: z.string().url(),
});
const RefreshBodySchema = z.object({ refreshToken: z.string().min(1) });

/**
 * OAuth + token endpoints. The flow is OAuth 2 with PKCE for a native app: the
 * desktop app calls `/url`, opens the returned URL in the system browser, catches
 * the loopback redirect, then posts the `code` + `codeVerifier` to `/exchange`.
 */
export function registerAuthRoutes(app: FastifyInstance, ctx: GatewayContext): void {
  // Build an authorization URL + PKCE pair for the desktop app to use.
  app.get('/v1/auth/:provider/url', async (req, reply) => {
    const provider = ProviderSchema.safeParse((req.params as { provider: string }).provider);
    if (!provider.success) return reply.code(400).send({ error: 'unknown provider' });
    const query = UrlQuerySchema.safeParse(req.query);
    if (!query.success) return reply.code(400).send({ error: 'redirectUri required' });

    const cfg = providerConfig(provider.data);
    if (!cfg.clientId) return reply.code(500).send({ error: 'provider not configured' });

    const pkce = generatePkce();
    const state = randomBytes(16).toString('hex');
    const url = buildAuthUrl(cfg, {
      redirectUri: query.data.redirectUri,
      state,
      codeChallenge: pkce.challenge,
    });
    // The desktop app keeps state + verifier and returns the verifier on exchange.
    return reply.send({ url, state, codeVerifier: pkce.verifier });
  });

  // Exchange an authorization code for Hearme access + refresh tokens.
  app.post('/v1/auth/:provider/exchange', async (req, reply) => {
    const provider = ProviderSchema.safeParse((req.params as { provider: string }).provider);
    if (!provider.success) return reply.code(400).send({ error: 'unknown provider' });
    const body = ExchangeBodySchema.safeParse(req.body);
    if (!body.success) return reply.code(400).send({ error: 'invalid request' });

    const cfg = providerConfig(provider.data);
    try {
      const identity = await exchangeCode(provider.data, cfg, body.data);
      const user = await upsertUser(ctx.db, {
        provider: identity.provider,
        providerSub: identity.sub,
        email: identity.email,
        displayName: identity.displayName,
        monthlyQuotaMinutes: ctx.config.monthlyQuotaMinutes,
      });
      const tokens = await issueTokens(ctx.tokens, user.id);
      return reply.send(tokens);
    } catch {
      // Never echo provider error bodies; they can carry tokens.
      return reply.code(401).send({ error: 'authentication failed' });
    }
  });

  // Exchange a refresh token for a new access + refresh pair.
  app.post('/v1/auth/refresh', async (req, reply) => {
    const body = RefreshBodySchema.safeParse(req.body);
    if (!body.success) return reply.code(400).send({ error: 'refreshToken required' });
    try {
      const userId = await verifyRefreshToken(ctx.tokens, body.data.refreshToken);
      const tokens = await issueTokens(ctx.tokens, userId);
      return reply.send(tokens);
    } catch {
      return reply.code(401).send({ error: 'invalid refresh token' });
    }
  });
}
