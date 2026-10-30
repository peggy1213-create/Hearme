import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import { config as loadEnv } from 'dotenv';
import type { Lang, Platform } from '@hearme/shared';
import { createSttAdapter, STT_VENDORS, type SttVendor } from '@hearme/stt';
import { hasFfmpeg } from './audio.js';
import { cleanReference, discoverCorpus, writeRefTemplates } from './corpus.js';
import { cer, wer, type ErrorRate } from './metrics.js';
import { renderCsv, renderMarkdown, type ReportMeta } from './report.js';
import { runSample, type RunOptions, type SampleRun } from './runner.js';

/**
 * Hearme STT benchmark CLI (Phase 0).
 *
 *   pnpm benchmark --data ./benchmark-data            run all vendors, write a report
 *   pnpm benchmark init-refs --data ./benchmark-data  write reference templates
 *   pnpm benchmark score --ref a.txt --hyp b.txt      quick two-file CER/WER check
 *
 * Reads vendor API keys from `.env` (gateway-only keys; this is a dev tool). If
 * the corpus folder is missing or has no audio, it stops and says so rather than
 * inventing data.
 */

// pnpm runs workspace scripts from the package dir; resolve paths against the
// directory the user actually invoked the command in.
const ROOT = process.env.INIT_CWD ?? process.cwd();
const resolveFromRoot = (p: string): string => (isAbsolute(p) ? p : resolve(ROOT, p));

interface Args {
  command: 'run' | 'init-refs' | 'score';
  data: string;
  out: string;
  vendors: SttVendor[];
  sampleRate: number;
  languages: Lang[];
  platform: Platform;
  ref?: string;
  hyp?: string;
}

function parseArgs(argv: string[]): Args {
  const args: Args = {
    command: 'run',
    data: './benchmark-data',
    out: './reports',
    vendors: [...STT_VENDORS],
    sampleRate: 16000,
    languages: ['en', 'zh-TW'],
    platform: 'other',
  };
  let i = 0;
  if (argv[0] && !argv[0].startsWith('--')) {
    const cmd = argv[0];
    if (cmd === 'init-refs' || cmd === 'score' || cmd === 'run') args.command = cmd;
    i = 1;
  }
  for (; i < argv.length; i++) {
    const flag = argv[i];
    const value = argv[i + 1];
    switch (flag) {
      case '--data':
        if (value) args.data = value;
        i++;
        break;
      case '--out':
        if (value) args.out = value;
        i++;
        break;
      case '--vendors':
        if (value)
          args.vendors = value
            .split(',')
            .map((v) => v.trim())
            .filter((v): v is SttVendor => (STT_VENDORS as readonly string[]).includes(v));
        i++;
        break;
      case '--sample-rate':
        if (value) args.sampleRate = Number(value);
        i++;
        break;
      case '--languages':
        if (value) args.languages = value.split(',').map((l) => l.trim()) as Lang[];
        i++;
        break;
      case '--ref':
        args.ref = value;
        i++;
        break;
      case '--hyp':
        args.hyp = value;
        i++;
        break;
      default:
        break;
    }
  }
  return args;
}

function fmt(label: string, r: ErrorRate): string {
  return `${label}: ${(r.rate * 100).toFixed(2)}%  (ref ${r.referenceLength}, sub ${r.substitutions}, del ${r.deletions}, ins ${r.insertions})`;
}

async function main(): Promise<void> {
  // Drop any lone `--` separators that pnpm forwards, so a subcommand like
  // `init-refs` is still seen as the first argument.
  const args = parseArgs(process.argv.slice(2).filter((a) => a !== '--'));
  loadEnv({ path: join(ROOT, '.env') });

  // Quick two-file score mode (no corpus needed).
  if (args.command === 'score') {
    if (!args.ref || !args.hyp) {
      console.error('usage: benchmark score --ref <file> --hyp <file>');
      process.exitCode = 1;
      return;
    }
    const ref = readFileSync(resolveFromRoot(args.ref), 'utf8');
    const hyp = readFileSync(resolveFromRoot(args.hyp), 'utf8');
    console.log(fmt('CER', cer(ref, hyp)));
    console.log(fmt('WER', wer(ref, hyp)));
    return;
  }

  const dataDir = resolveFromRoot(args.data);
  const samples = discoverCorpus(dataDir);

  if (samples.length === 0) {
    console.error(
      `No audio files found in ${dataDir}.\n` +
        'Add your benchmark meetings there (wav/mp3/m4a/flac/ogg/opus/webm) and a\n' +
        '<name>.txt reference beside each one, then re-run. Stopping — not inventing data.',
    );
    process.exitCode = 1;
    return;
  }

  if (args.command === 'init-refs') {
    const written = writeRefTemplates(dataDir);
    if (written.length === 0) {
      console.log(`All ${samples.length} sample(s) already have a reference transcript.`);
    } else {
      console.log(`Wrote ${written.length} reference template(s):`);
      for (const p of written) console.log(`  ${p}`);
      console.log('Fill each one in with the exact words spoken, then run the benchmark.');
    }
    return;
  }

  // --- run ---
  const withRefs = samples.map((s) => ({
    ...s,
    // An all-comments template cleans down to '' — treat that as no reference.
    reference: s.reference ? cleanReference(s.reference) || null : null,
  }));
  const missing = withRefs.filter((s) => !s.reference);
  if (missing.length > 0) {
    console.warn(
      `Warning: ${missing.length} of ${withRefs.length} sample(s) have no reference ` +
        `transcript and will not be scored: ${missing.map((s) => s.name).join(', ')}.\n` +
        `Run 'benchmark init-refs --data ${args.data}' to create templates.`,
    );
  }

  const ffmpegAvailable = await hasFfmpeg();
  if (!ffmpegAvailable) {
    console.warn('Warning: ffmpeg not found on PATH — the streaming (live) pass will be skipped.');
  }

  const runs: SampleRun[] = [];
  for (const vendor of args.vendors) {
    const adapter = createSttAdapter(vendor);
    const opts: RunOptions = {
      sampleRate: args.sampleRate,
      languages: args.languages,
      platform: args.platform,
      skipStream: vendor === 'whisper',
      streamSkipReason: vendor === 'whisper' ? 'whisper is batch-only' : undefined,
    };
    for (const sample of withRefs) {
      console.log(`[${vendor}] ${sample.name} …`);
      const run = await runSample(vendor, adapter, sample, opts);
      if (!run.batch.ok) console.log(`  batch: ${run.batch.error}`);
      if (!run.stream.ok && !opts.skipStream) console.log(`  live:  ${run.stream.error}`);
      runs.push(run);
    }
  }

  const date = new Date().toISOString().slice(0, 10);
  const meta: ReportMeta = {
    date,
    dataDir: args.data,
    sampleRate: args.sampleRate,
    languages: args.languages,
    vendors: args.vendors,
    samples: withRefs.map((s) => {
      const run = runs.find((r) => r.sample === s.name);
      return {
        name: s.name,
        durationSec: run?.durationSec ?? 0,
        hasReference: s.reference !== null,
      };
    }),
  };

  const outDir = resolveFromRoot(args.out);
  mkdirSync(outDir, { recursive: true });
  const mdPath = join(outDir, `benchmark-${date}.md`);
  const csvPath = join(outDir, `benchmark-${date}.csv`);
  writeFileSync(mdPath, renderMarkdown(runs, meta), 'utf8');
  writeFileSync(csvPath, renderCsv(runs), 'utf8');

  console.log(`\nReport written:\n  ${mdPath}\n  ${csvPath}`);
}

main().catch((err: unknown) => {
  console.error(err instanceof Error ? err.stack : err);
  process.exitCode = 1;
});
