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
 * Soniox adapter (stub). Soniox streams with diarization and handles Mandarin–
 * English code-switching, a candidate for the Phase 0 benchmark. Filled in later.
 */
export class SonioxAdapter implements SttAdapter {
  readonly name = 'soniox';
  readonly supportsCodeSwitching = true;

  openStream(_options: SttStreamOptions, _handlers: SttStreamHandlers): Promise<SttStream> {
    throw new SttNotImplementedError(this.name, 'openStream');
  }

  async transcribeBatch(_options: SttBatchOptions): Promise<TranscriptSegment[]> {
    throw new SttNotImplementedError(this.name, 'transcribeBatch');
  }
}
