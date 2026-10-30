export * from './adapter.js';
export { SonioxAdapter } from './vendors/soniox.js';
export { AssemblyAIAdapter } from './vendors/assemblyai.js';
export { DeepgramAdapter } from './vendors/deepgram.js';

import type { SttAdapter } from './adapter.js';
import { AssemblyAIAdapter } from './vendors/assemblyai.js';
import { DeepgramAdapter } from './vendors/deepgram.js';
import { SonioxAdapter } from './vendors/soniox.js';

/** Known vendor ids. */
export type SttVendor = 'soniox' | 'assemblyai' | 'deepgram';

/** Construct an adapter by vendor id. */
export function createSttAdapter(vendor: SttVendor): SttAdapter {
  switch (vendor) {
    case 'soniox':
      return new SonioxAdapter();
    case 'assemblyai':
      return new AssemblyAIAdapter();
    case 'deepgram':
      return new DeepgramAdapter();
  }
}
