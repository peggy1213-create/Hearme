import { describe, expect, it } from 'vitest';
import { createSttAdapter, STT_VENDORS, SttNotImplementedError } from './index.js';

describe('createSttAdapter', () => {
  it('builds each known vendor', () => {
    expect(createSttAdapter('soniox').name).toBe('soniox');
    expect(createSttAdapter('assemblyai').name).toBe('assemblyai');
    expect(createSttAdapter('deepgram').name).toBe('deepgram');
    expect(createSttAdapter('whisper').name).toBe('whisper');
  });

  it('lists every vendor in STT_VENDORS', () => {
    expect([...STT_VENDORS]).toEqual(['soniox', 'assemblyai', 'deepgram', 'whisper']);
  });

  it('reports code-switching support per vendor', () => {
    // Soniox, AssemblyAI (multilingual streaming) and Deepgram (nova-3 language=multi)
    // all handle Mandarin–English code-switching; whisper auto-detects per segment.
    expect(createSttAdapter('soniox').supportsCodeSwitching).toBe(true);
    expect(createSttAdapter('assemblyai').supportsCodeSwitching).toBe(true);
    expect(createSttAdapter('deepgram').supportsCodeSwitching).toBe(true);
  });

  it('whisper has no streaming mode', () => {
    expect(() => createSttAdapter('whisper').openStream({} as never, {} as never)).toThrow(
      SttNotImplementedError,
    );
  });

  it('cloud adapters fail clearly when their key is missing', async () => {
    const adapter = createSttAdapter('soniox', { apiKey: '' });
    await expect(
      adapter.transcribeBatch({ meetingId: 'm', languages: ['en'], audioPath: 'x.wav' }),
    ).rejects.toThrow(/SONIOX_API_KEY/);
  });
});
