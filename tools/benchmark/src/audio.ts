import { spawn } from 'node:child_process';

/**
 * Audio decoding for the streaming simulation. The benchmark feeds raw PCM to the
 * streaming adapters, so we decode each input file once (via ffmpeg) to mono
 * little-endian 16-bit PCM at the target sample rate.
 *
 * ffmpeg must be on PATH. If it is missing, {@link decodePcm16} throws a clear
 * error and the caller skips the streaming pass (batch mode needs no decode — the
 * adapters upload the file as-is).
 */

export interface DecodedAudio {
  /** Raw mono s16le PCM at `sampleRate`. */
  pcm: Buffer;
  /** Duration in seconds, derived from the PCM length. */
  durationSec: number;
  sampleRate: number;
}

const FFMPEG = process.env.FFMPEG_BIN ?? 'ffmpeg';

/** Whether ffmpeg can be launched. */
export function hasFfmpeg(): Promise<boolean> {
  return new Promise((resolve) => {
    const proc = spawn(FFMPEG, ['-version']);
    proc.on('error', () => resolve(false));
    proc.on('close', (code) => resolve(code === 0));
  });
}

/** Decode any audio file to mono s16le PCM at `sampleRate`. */
export function decodePcm16(audioPath: string, sampleRate: number): Promise<DecodedAudio> {
  return new Promise((resolve, reject) => {
    const proc = spawn(FFMPEG, [
      '-hide_banner',
      '-loglevel',
      'error',
      '-i',
      audioPath,
      '-f',
      's16le',
      '-acodec',
      'pcm_s16le',
      '-ac',
      '1',
      '-ar',
      String(sampleRate),
      '-',
    ]);
    const chunks: Buffer[] = [];
    let stderr = '';
    proc.stdout.on('data', (d: Buffer) => chunks.push(d));
    proc.stderr.on('data', (d: Buffer) => (stderr += d.toString()));
    proc.on('error', (err) =>
      reject(new Error(`ffmpeg could not be launched (${FFMPEG}): ${err.message}`)),
    );
    proc.on('close', (code) => {
      if (code !== 0) {
        reject(
          new Error(`ffmpeg failed to decode ${audioPath}: ${stderr.trim() || `exit ${code}`}`),
        );
        return;
      }
      const pcm = Buffer.concat(chunks);
      resolve({ pcm, durationSec: pcm.length / (2 * sampleRate), sampleRate });
    });
  });
}
