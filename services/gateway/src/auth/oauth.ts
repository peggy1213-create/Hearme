import { createHash, randomBytes } from 'node:crypto';
import { decodeJwt } from 'jose';

/**
 * OAuth 2 with PKCE for Google and Microsoft. The desktop app opens the system
 * browser, the provider redirects to a loopback URL the app listens on, and the
 * app hands the `code` back to the gateway to exchange. The gateway holds the
 * client secret; the desktop app never does.
 *
 * Endpoints/claims checked 2026-11-03:
 * - Google: https://developers.google.com/identity/protocols/oauth2/native-app
 * - Microsoft: https://learn.microsoft.com/entra/identity-platform/v2-oauth2-auth-code-flow
 */

export type OAuthProvider = 'google' | 'microsoft';

export interface ProviderConfig {
  clientId: string;
  clientSecret: string;
  authEndpoint: string;
  tokenEndpoint: string;
  scopes: string[];
}

export interface OAuthIdentity {
  provider: OAuthProvider;
  /** Stable subject id from the provider. */
  sub: string;
  email?: string;
  displayName?: string;
}

/** PKCE pair: the verifier stays on the desktop app; the challenge goes in the URL. */
export interface PkcePair {
  verifier: string;
  challenge: string;
}

const base64url = (buf: Buffer): string =>
  buf.toString('base64').replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');

/** Generate a PKCE verifier and its S256 challenge. */
export function generatePkce(): PkcePair {
  const verifier = base64url(randomBytes(32));
  const challenge = base64url(createHash('sha256').update(verifier).digest());
  return { verifier, challenge };
}

export function providerConfig(
  provider: OAuthProvider,
  env: NodeJS.ProcessEnv = process.env,
): ProviderConfig {
  if (provider === 'google') {
    return {
      clientId: env.GOOGLE_OAUTH_CLIENT_ID ?? '',
      clientSecret: env.GOOGLE_OAUTH_CLIENT_SECRET ?? '',
      authEndpoint: 'https://accounts.google.com/o/oauth2/v2/auth',
      tokenEndpoint: 'https://oauth2.googleapis.com/token',
      scopes: ['openid', 'email', 'profile'],
    };
  }
  const tenant = env.MICROSOFT_OAUTH_TENANT ?? 'common';
  return {
    clientId: env.MICROSOFT_OAUTH_CLIENT_ID ?? '',
    clientSecret: env.MICROSOFT_OAUTH_CLIENT_SECRET ?? '',
    authEndpoint: `https://login.microsoftonline.com/${tenant}/oauth2/v2.0/authorize`,
    tokenEndpoint: `https://login.microsoftonline.com/${tenant}/oauth2/v2.0/token`,
    scopes: ['openid', 'email', 'profile'],
  };
}

export interface AuthUrlInput {
  redirectUri: string;
  state: string;
  codeChallenge: string;
}

/** Build the authorization URL to open in the system browser. */
export function buildAuthUrl(cfg: ProviderConfig, input: AuthUrlInput): string {
  const params = new URLSearchParams({
    client_id: cfg.clientId,
    response_type: 'code',
    redirect_uri: input.redirectUri,
    scope: cfg.scopes.join(' '),
    state: input.state,
    code_challenge: input.codeChallenge,
    code_challenge_method: 'S256',
    access_type: 'offline',
    prompt: 'consent',
  });
  return `${cfg.authEndpoint}?${params.toString()}`;
}

interface TokenResponse {
  id_token?: string;
  error?: string;
  error_description?: string;
}

interface IdTokenClaims {
  sub?: string;
  oid?: string;
  email?: string;
  preferred_username?: string;
  name?: string;
}

export interface ExchangeInput {
  code: string;
  codeVerifier: string;
  redirectUri: string;
}

/**
 * Exchange an authorization code for the provider's id_token and read the user's
 * identity from it. The id_token arrives directly from the token endpoint over
 * TLS, so per OIDC §3.1.3.7 its signature need not be re-verified here; we only
 * decode the claims.
 */
export async function exchangeCode(
  provider: OAuthProvider,
  cfg: ProviderConfig,
  input: ExchangeInput,
): Promise<OAuthIdentity> {
  const body = new URLSearchParams({
    client_id: cfg.clientId,
    client_secret: cfg.clientSecret,
    grant_type: 'authorization_code',
    code: input.code,
    redirect_uri: input.redirectUri,
    code_verifier: input.codeVerifier,
  });
  const res = await fetch(cfg.tokenEndpoint, {
    method: 'POST',
    headers: { 'Content-Type': 'application/x-www-form-urlencoded' },
    body,
  });
  const json = (await res.json()) as TokenResponse;
  if (!res.ok || !json.id_token) {
    throw new Error(`oauth token exchange failed: ${json.error ?? res.status}`);
  }
  const claims = decodeJwt(json.id_token) as IdTokenClaims;
  const sub = claims.sub ?? claims.oid;
  if (!sub) throw new Error('id_token has no subject');
  return {
    provider,
    sub,
    email: claims.email ?? claims.preferred_username,
    displayName: claims.name,
  };
}
