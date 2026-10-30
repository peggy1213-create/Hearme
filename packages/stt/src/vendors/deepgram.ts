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
 * Deepgram adapter (stub). Nova-3 streams with diarization; a Phase 0 benchmark
 * candidate. Filled in later.
 */
export class DeepgramAdapter implements SttAdapter {
  readonly name = 'deepgram';
  readonly supportsCodeSwitching = false;

  openStream(_options: SttStreamOptions, _handlers: SttStreamHandlers): Promise<SttStream> {
    throw new SttNotImplementedError(this.name, 'openStream');
  }

  async transcribeBatch(_options: SttBatchOptions): Promise<TranscriptSegment[]> {
    throw new SttNotImplementedError(this.name, 'transcribeBatch');
  }
}
