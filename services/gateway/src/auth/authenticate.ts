import type { GatewayContext } from '../context.js';
import { upsertUser } from '../db/store.js';
import { verifyAccessToken } from './tokens.js';

/** An authentication failure, mapped to the protocol's `unauthorized` error. */
export class AuthError extends Error {
  constructor(message: string) {
    super(message);
    this.name = 'AuthError';
  }
}

/** Extract a bearer token from an `Authorization` header or a `token` query value. */
export function extractToken(
  authHeader: string | undefined,
  queryToken: string | undefined,
): string | undefined {
  if (authHeader?.toLowerCase().startsWith('bearer ')) return authHeader.slice(7).trim();
  return queryToken?.trim() || undefined;
}

/**
 * Resolve a request to a user id. In development a configured bypass token maps to
 * a stable `dev` user (created on first use); otherwise the token must be a valid
 * access JWT. Throws {@link AuthError} on any failure.
 */
export async function resolveUserId(
  ctx: GatewayContext,
  authHeader: string | undefined,
  queryToken: string | undefined,
): Promise<string> {
  const token = extractToken(authHeader, queryToken);
  if (!token) throw new AuthError('missing token');

  if (ctx.config.isDev && ctx.config.devBypassToken && token === ctx.config.devBypassToken) {
    const user = await upsertUser(ctx.db, {
      provider: 'dev',
      providerSub: 'dev-user',
      email: 'dev@localhost',
      displayName: 'Dev User',
      monthlyQuotaMinutes: ctx.config.monthlyQuotaMinutes,
    });
    return user.id;
  }

  try {
    return await verifyAccessToken(ctx.tokens, token);
  } catch {
    throw new AuthError('invalid or expired token');
  }
}
