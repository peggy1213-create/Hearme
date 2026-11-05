import { SignJWT, jwtVerify } from 'jose';

/**
 * Short-lived JWT access tokens plus longer-lived refresh tokens. Both are signed
 * with `JWT_SECRET`; refresh tokens are stateless (distinguished by the `aud`
 * claim) so the gateway needs no token table — only `users` and `usage`.
 */

const ISSUER = 'hearme-gateway';
const ACCESS_AUD = 'access';
const REFRESH_AUD = 'refresh';

export interface TokenConfig {
  secret: Uint8Array;
  accessTtl: string;
  refreshTtl: string;
}

export function tokenConfigFromEnv(env: NodeJS.ProcessEnv = process.env): TokenConfig {
  // In production a secret is mandatory; dev/test fall back to a known insecure one.
  const secret = env.JWT_SECRET ?? (env.NODE_ENV === 'production' ? '' : 'dev-insecure-change-me');
  if (!secret) throw new Error('JWT_SECRET is not set');
  return {
    secret: new TextEncoder().encode(secret),
    accessTtl: env.JWT_ACCESS_TTL ?? '15m',
    refreshTtl: env.JWT_REFRESH_TTL ?? '30d',
  };
}

export interface IssuedTokens {
  accessToken: string;
  refreshToken: string;
  tokenType: 'Bearer';
}

/** Mint a fresh access + refresh pair for a user. */
export async function issueTokens(cfg: TokenConfig, userId: string): Promise<IssuedTokens> {
  const accessToken = await new SignJWT({})
    .setProtectedHeader({ alg: 'HS256' })
    .setSubject(userId)
    .setIssuer(ISSUER)
    .setAudience(ACCESS_AUD)
    .setIssuedAt()
    .setExpirationTime(cfg.accessTtl)
    .sign(cfg.secret);

  const refreshToken = await new SignJWT({})
    .setProtectedHeader({ alg: 'HS256' })
    .setSubject(userId)
    .setIssuer(ISSUER)
    .setAudience(REFRESH_AUD)
    .setIssuedAt()
    .setExpirationTime(cfg.refreshTtl)
    .sign(cfg.secret);

  return { accessToken, refreshToken, tokenType: 'Bearer' };
}

async function verify(cfg: TokenConfig, token: string, audience: string): Promise<string> {
  const { payload } = await jwtVerify(token, cfg.secret, { issuer: ISSUER, audience });
  if (!payload.sub) throw new Error('token has no subject');
  return payload.sub;
}

/** Verify an access token, returning its user id. Throws if invalid/expired. */
export function verifyAccessToken(cfg: TokenConfig, token: string): Promise<string> {
  return verify(cfg, token, ACCESS_AUD);
}

/** Verify a refresh token, returning its user id. Throws if invalid/expired. */
export function verifyRefreshToken(cfg: TokenConfig, token: string): Promise<string> {
  return verify(cfg, token, REFRESH_AUD);
}
