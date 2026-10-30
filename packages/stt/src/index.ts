export * from './adapter.js';
export { SonioxAdapter } from './vendors/soniox.js';
export { AssemblyAIAdapter } from './vendors/assemblyai.js';
export { DeepgramAdapter } from './vendors/deepgram.js';
export { WhisperAdapter } from './vendors/whisper.js';

import type { SttAdapter } from './adapter.js';
import { AssemblyAIAdapter } from './vendors/assemblyai.js';
import { DeepgramAdapter } from './vendors/deepgram.js';
import { SonioxAdapter } from './vendors/soniox.js';
import { WhisperAdapter } from './vendors/whisper.js';

/** Known vendor ids. `whisper` is the local (on-device) baseline. */
export type SttVendor = 'soniox' | 'assemblyai' | 'deepgram' | 'whisper';

/** Every vendor id, in the order the benchmark reports them. */
export const STT_VENDORS: readonly SttVendor[] = ['soniox', 'assemblyai', 'deepgram', 'whisper'];

/** Per-vendor construction config. Cloud vendors read their key from `.env` when omitted. */
export interface SttAdapterConfig {
  apiKey?: string;
  /** whisper only. */
  model?: string;
  /** whisper only. */
  pythonBin?: string;
}

/** Construct an adapter by vendor id. */
export function createSttAdapter(vendor: SttVendor, config: SttAdapterConfig = {}): SttAdapter {
  switch (vendor) {
    case 'soniox':
      return new SonioxAdapter(config);
    case 'assemblyai':
      return new AssemblyAIAdapter(config);
    case 'deepgram':
      return new DeepgramAdapter(config);
    case 'whisper':
      return new WhisperAdapter(config);
  }
}
