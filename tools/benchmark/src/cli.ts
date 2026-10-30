import { readFileSync } from 'node:fs';
import { cer, wer, type ErrorRate } from './metrics.js';

/**
 * STT benchmark CLI (skeleton). Compares a hypothesis transcript against a
 * reference and prints CER (Chinese) and WER (English).
 *
 *   hearme-benchmark --ref ref.txt --hyp hyp.txt [--metric cer|wer]
 *
 * Later this grows into a harness that runs each vendor adapter over a corpus of
 * real Taiwanese meetings (docs/plan.md, Phase 0).
 */

interface Args {
  ref?: string;
  hyp?: string;
  metric: 'cer' | 'wer' | 'both';
}

function parseArgs(argv: string[]): Args {
  const args: Args = { metric: 'both' };
  for (let i = 0; i < argv.length; i++) {
    const flag = argv[i];
    const value = argv[i + 1];
    if (flag === '--ref') {
      args.ref = value;
      i++;
    } else if (flag === '--hyp') {
      args.hyp = value;
      i++;
    } else if (flag === '--metric') {
      if (value === 'cer' || value === 'wer' || value === 'both') args.metric = value;
      i++;
    }
  }
  return args;
}

function format(label: string, r: ErrorRate): string {
  const pct = (r.rate * 100).toFixed(2);
  return `${label}: ${pct}%  (ref ${r.referenceLength}, sub ${r.substitutions}, del ${r.deletions}, ins ${r.insertions})`;
}

function main(): void {
  const args = parseArgs(process.argv.slice(2));
  if (!args.ref || !args.hyp) {
    console.error('usage: hearme-benchmark --ref <file> --hyp <file> [--metric cer|wer|both]');
    process.exitCode = 1;
    return;
  }

  const reference = readFileSync(args.ref, 'utf8');
  const hypothesis = readFileSync(args.hyp, 'utf8');

  if (args.metric === 'cer' || args.metric === 'both') {
    console.log(format('CER', cer(reference, hypothesis)));
  }
  if (args.metric === 'wer' || args.metric === 'both') {
    console.log(format('WER', wer(reference, hypothesis)));
  }
}

main();
