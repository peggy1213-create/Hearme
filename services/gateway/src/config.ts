import type { SttVendor } from '@hearme/stt';

/** Gateway configuration, read once from the environment at startup. */
export interface GatewayConfig {
  nodeEnv: string;
  isDev: boolean;
  /** Primary live-pass vendor and its automatic fallback (ADR 0005, provisional). */
  liveVendor: SttVendor;
  fallbackVendor: SttVendor;
  /** Final (batch) pass vendor; may differ from the live vendor (ADR 0005). */
  batchVendor: SttVendor;
  /** Dev-only bypass token; honoured only when `isDev`. */
  devBypassToken: string | undefined;
  /** Default monthly transcription-minute quota for new users. */
  monthlyQuotaMinutes: number;
  /** How long a dropped meeting stays resumable in memory, in ms. */
  resumeWindowMs: number;
}

const VENDORS = new Set<SttVendor>(['soniox', 'assemblyai', 'deepgram', 'whisper', 'mock']);

function vendor(value: string | undefined, fallback: SttVendor): SttVendor {
  return value && VENDORS.has(value as SttVendor) ? (value as SttVendor) : fallback;
}

export function loadConfig(env: NodeJS.ProcessEnv = process.env): GatewayConfig {
  const nodeEnv = env.NODE_ENV ?? 'development';
  const isDev = nodeEnv === 'development';
  return {
    nodeEnv,
    isDev,
    liveVendor: vendor(env.STT_LIVE_VENDOR, 'soniox'),
    fallbackVendor: vendor(env.STT_FALLBACK_VENDOR, 'assemblyai'),
    batchVendor: vendor(env.STT_BATCH_VENDOR, vendor(env.STT_LIVE_VENDOR, 'soniox')),
    devBypassToken: isDev ? (env.DEV_BYPASS_TOKEN ?? 'dev-bypass') : undefined,
    monthlyQuotaMinutes: Number(env.MONTHLY_QUOTA_MINUTES ?? 1200),
    resumeWindowMs: Number(env.RESUME_WINDOW_MS ?? 3 * 60_000),
  };
}
