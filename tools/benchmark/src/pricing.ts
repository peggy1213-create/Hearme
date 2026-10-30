import type { SttVendor } from '@hearme/stt';

/**
 * Published vendor pricing, fetched 2026-10-30. These feed the "estimated cost
 * per audio hour" column of the benchmark report. Prices change — re-check the
 * sources before quoting them in the ADR.
 *
 * All figures are the diarization-included effective rate for the mode, since the
 * benchmark runs every vendor with diarization on.
 */
export interface VendorPricing {
  /** Live/streaming USD per audio hour (diarization included). */
  liveUsdPerHour: number;
  /** Batch/pre-recorded USD per audio hour (diarization included). */
  batchUsdPerHour: number;
  source: string;
}

export const PRICING: Record<SttVendor, VendorPricing> = {
  soniox: {
    liveUsdPerHour: 0.12,
    batchUsdPerHour: 0.1,
    // soniox.com/pricing (updated Jun 2026): real-time $0.12/hr, async $0.10/hr;
    // diarization + language identification included.
    source: 'soniox.com/pricing (2026-06)',
  },
  assemblyai: {
    // Streaming $0.15/hr + streaming diarization $0.12/hr.
    liveUsdPerHour: 0.15 + 0.12,
    // Async Universal-3.5 Pro $0.21/hr + async diarization (standard) $0.02/hr.
    batchUsdPerHour: 0.21 + 0.02,
    source: 'assemblyai.com/pricing (2026-10-27)',
  },
  deepgram: {
    // Nova-3 streaming multilingual $0.0092/min + diarization $0.0020/min.
    liveUsdPerHour: (0.0092 + 0.002) * 60,
    // Nova-3 pre-recorded multilingual $0.0052/min, diarization included.
    batchUsdPerHour: 0.0052 * 60,
    source: 'deepgram.com/pricing (PAYG, 2026-10)',
  },
  whisper: {
    // Local/on-device: no per-hour vendor fee, only your own compute.
    liveUsdPerHour: 0,
    batchUsdPerHour: 0,
    source: 'local (faster-whisper); compute cost only',
  },
};

/** Estimated USD for `audioSeconds` of audio at a per-hour rate. */
export function estimateCost(usdPerHour: number, audioSeconds: number): number {
  return (usdPerHour * audioSeconds) / 3600;
}
