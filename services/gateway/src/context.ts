import { FailoverAdapter, createSttAdapter, type SttAdapter } from '@hearme/stt';
import type { GatewayConfig } from './config.js';
import type { GatewayDatabase } from './db/client.js';
import { gatewayMetrics } from './otel.js';
import type { TokenConfig } from './auth/tokens.js';

/**
 * Everything a request handler needs, assembled once. Tests override
 * `buildLiveAdapter` / `buildBatchAdapter` (to inject the mock vendor) and `db`
 * (to inject PGlite) without touching the rest.
 */
export interface GatewayContext {
  config: GatewayConfig;
  tokens: TokenConfig;
  db: GatewayDatabase;
  /** Build the live-pass adapter: the primary vendor with automatic fallback. */
  buildLiveAdapter: () => SttAdapter;
  /** Build the final-pass (batch) adapter. */
  buildBatchAdapter: () => SttAdapter;
}

/** Default live adapter: configured primary + fallback, counting failovers. */
export function defaultLiveAdapter(config: GatewayConfig): SttAdapter {
  return new FailoverAdapter(
    createSttAdapter(config.liveVendor),
    createSttAdapter(config.fallbackVendor),
    (from, to) => {
      gatewayMetrics.vendorErrors.add(1, { vendor: from, failedOverTo: to });
    },
  );
}
