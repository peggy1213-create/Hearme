import { readdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { basename, extname, join } from 'node:path';

/**
 * Corpus discovery for the STT benchmark. A corpus is a folder of audio files,
 * each paired with a reference transcript named `<audio-basename>.txt` beside it.
 */

const AUDIO_EXTENSIONS = new Set(['.wav', '.mp3', '.m4a', '.flac', '.ogg', '.opus', '.webm']);

export interface CorpusSample {
  /** File basename without extension, used as the sample's display name. */
  name: string;
  audioPath: string;
  refPath: string;
  /** Reference transcript text, or `null` when the `.txt` is missing. */
  reference: string | null;
}

/** Find every audio file in `dataDir` and pair it with its reference transcript. */
export function discoverCorpus(dataDir: string): CorpusSample[] {
  if (!existsSync(dataDir)) return [];
  const samples: CorpusSample[] = [];
  for (const entry of readdirSync(dataDir).sort()) {
    const ext = extname(entry).toLowerCase();
    if (!AUDIO_EXTENSIONS.has(ext)) continue;
    const name = basename(entry, ext);
    const audioPath = join(dataDir, entry);
    const refPath = join(dataDir, `${name}.txt`);
    const reference = existsSync(refPath) ? readFileSync(refPath, 'utf8').trim() : null;
    samples.push({ name, audioPath, refPath, reference });
  }
  return samples;
}

const TEMPLATE = `# Reference transcript for {name}
#
# Type the EXACT words spoken, in order, one meeting between the markers below.
# - Chinese in Traditional characters with Taiwan wording (the benchmark converts
#   Simplified before scoring, but write Traditional).
# - Keep English words as spoken; code-switching within a line is fine.
# - Lines starting with # are ignored.
# - Speaker labels and timestamps are not needed for scoring.
# ---- transcript below this line ----
`;

/**
 * Write a reference-transcript template next to every audio file that does not
 * already have one. Returns the paths written.
 */
export function writeRefTemplates(dataDir: string): string[] {
  const written: string[] = [];
  for (const sample of discoverCorpus(dataDir)) {
    if (existsSync(sample.refPath)) continue;
    writeFileSync(sample.refPath, TEMPLATE.replace('{name}', sample.name), 'utf8');
    written.push(sample.refPath);
  }
  return written;
}

/** Strip comment lines (`#…`) and collapse whitespace in a reference transcript. */
export function cleanReference(raw: string): string {
  return raw
    .split(/\r?\n/)
    .filter((line) => !line.trimStart().startsWith('#'))
    .join(' ')
    .replace(/\s+/g, ' ')
    .trim();
}
