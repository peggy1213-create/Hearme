import { readFile } from 'node:fs/promises';

/**
 * Minimal RIFF/WAVE reader for the replay tool: enough to stream a recording
 * through the gateway, with no ffmpeg dependency. Handles 16-bit PCM, downmixing
 * stereo to mono. Other formats throw a clear error.
 */
export interface WavAudio {
  sampleRate: number;
  /** 16-bit mono samples. */
  samples: Int16Array;
}

export async function readWav(path: string): Promise<WavAudio> {
  const buf = await readFile(path);
  if (buf.toString('ascii', 0, 4) !== 'RIFF' || buf.toString('ascii', 8, 12) !== 'WAVE') {
    throw new Error(`${path}: not a WAV file`);
  }

  let offset = 12;
  let sampleRate = 0;
  let channels = 1;
  let bitsPerSample = 16;
  let audioFormat = 1;
  let dataStart = -1;
  let dataLen = 0;

  while (offset + 8 <= buf.length) {
    const id = buf.toString('ascii', offset, offset + 4);
    const size = buf.readUInt32LE(offset + 4);
    const body = offset + 8;
    if (id === 'fmt ') {
      audioFormat = buf.readUInt16LE(body);
      channels = buf.readUInt16LE(body + 2);
      sampleRate = buf.readUInt32LE(body + 4);
      bitsPerSample = buf.readUInt16LE(body + 14);
    } else if (id === 'data') {
      dataStart = body;
      dataLen = size;
    }
    offset = body + size + (size % 2); // chunks are word-aligned
  }

  if (audioFormat !== 1 || bitsPerSample !== 16) {
    throw new Error(`${path}: only 16-bit PCM WAV is supported`);
  }
  if (dataStart < 0) throw new Error(`${path}: no data chunk`);

  const frameCount = Math.floor(dataLen / 2 / channels);
  const mono = new Int16Array(frameCount);
  for (let i = 0; i < frameCount; i++) {
    let sum = 0;
    for (let c = 0; c < channels; c++) {
      sum += buf.readInt16LE(dataStart + (i * channels + c) * 2);
    }
    mono[i] = Math.round(sum / channels);
  }
  return { sampleRate, samples: mono };
}
