import type { TranscriptSegment } from '@hearme/shared';
import type {
  SttAdapter,
  SttBatchOptions,
  SttStream,
  SttStreamHandlers,
  SttStreamOptions,
} from '../adapter.js';
import { SttNotImplementedError } from '../adapter.js';

/**
 * AssemblyAI adapter (stub). Universal-3.5 Pro Realtime streams with diarization
 * and handles code-switching; a Phase 0 benchmark candidate. Filled in later.
 */
export class AssemblyAIAdapter implements SttAdapter {
  readonly name = 'assemblyai';
  readonly supportsCodeSwitching = true;

  openStream(_options: SttStreamOptions, _handlers: SttStreamHandlers): Promise<SttStream> {
    throw new SttNotImplementedError(this.name, 'openStream');
  }

  async transcribeBatch(_options: SttBatchOptions): Promise<TranscriptSegment[]> {
    throw new SttNotImplementedError(this.name, 'transcribeBatch');
  }
}
