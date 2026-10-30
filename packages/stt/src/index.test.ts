import { describe, expect, it } from 'vitest';
import { createSttAdapter, SttNotImplementedError } from './index.js';

describe('createSttAdapter', () => {
  it('builds each known vendor', () => {
    expect(createSttAdapter('soniox').name).toBe('soniox');
    expect(createSttAdapter('assemblyai').name).toBe('assemblyai');
    expect(createSttAdapter('deepgram').name).toBe('deepgram');
  });

  it('reports code-switching support per vendor', () => {
    expect(createSttAdapter('soniox').supportsCodeSwitching).toBe(true);
    expect(createSttAdapter('deepgram').supportsCodeSwitching).toBe(false);
  });

  it('batch transcription is not implemented yet', async () => {
    const adapter = createSttAdapter('soniox');
    await expect(
      adapter.transcribeBatch({ meetingId: 'm', languages: ['en'], audioPath: 'x.ogg' }),
    ).rejects.toBeInstanceOf(SttNotImplementedError);
  });
});
