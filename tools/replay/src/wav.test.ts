import { mkdtemp, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';
import { readWav } from './wav.js';

/** Build a minimal 16-bit PCM WAV in memory. */
function makeWav(sampleRate: number, channels: number, samples: number[]): Buffer {
  const bytesPerSample = 2;
  const dataLen = samples.length * bytesPerSample;
  const buf = Buffer.alloc(44 + dataLen);
  buf.write('RIFF', 0);
  buf.writeUInt32LE(36 + dataLen, 4);
  buf.write('WAVE', 8);
  buf.write('fmt ', 12);
  buf.writeUInt32LE(16, 16);
  buf.writeUInt16LE(1, 20); // PCM
  buf.writeUInt16LE(channels, 22);
  buf.writeUInt32LE(sampleRate, 24);
  buf.writeUInt32LE(sampleRate * channels * bytesPerSample, 28);
  buf.writeUInt16LE(channels * bytesPerSample, 32);
  buf.writeUInt16LE(16, 34);
  buf.write('data', 36);
  buf.writeUInt32LE(dataLen, 40);
  samples.forEach((s, i) => buf.writeInt16LE(s, 44 + i * 2));
  return buf;
}

const dir = await mkdtemp(join(tmpdir(), 'hearme-wav-'));

describe('readWav', () => {
  it('reads a mono 16-bit PCM file', async () => {
    const path = join(dir, 'mono.wav');
    await writeFile(path, makeWav(16000, 1, [0, 100, -100, 200]));
    const wav = await readWav(path);
    expect(wav.sampleRate).toBe(16000);
    expect([...wav.samples]).toEqual([0, 100, -100, 200]);
  });

  it('downmixes stereo to mono', async () => {
    const path = join(dir, 'stereo.wav');
    // L/R interleaved: (0,100) -> 50, (200,-200) -> 0
    await writeFile(path, makeWav(16000, 2, [0, 100, 200, -200]));
    const wav = await readWav(path);
    expect([...wav.samples]).toEqual([50, 0]);
  });

  it('rejects a non-WAV file', async () => {
    const path = join(dir, 'bad.wav');
    await writeFile(path, Buffer.from('not a wav at all'));
    await expect(readWav(path)).rejects.toThrow(/not a WAV/);
  });
});

afterAll(async () => {
  await import('node:fs/promises').then((fs) => fs.rm(dir, { recursive: true, force: true }));
});
