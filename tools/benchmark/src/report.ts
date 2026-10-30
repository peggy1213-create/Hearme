import type { SttVendor } from '@hearme/stt';
import {
  cerChineseOnly,
  mixedErrorRate,
  percentile,
  werEnglishOnly,
  type ErrorRate,
} from './metrics.js';
import { estimateCost, PRICING } from './pricing.js';
import type { PassResult, SampleRun, StreamPassResult } from './runner.js';

/**
 * Renders the benchmark results as a Markdown report (tables) and a CSV. Metrics
 * are computed here from the raw hypotheses the runner collected, scored against
 * each sample's reference transcript.
 */

export interface ReportMeta {
  date: string;
  dataDir: string;
  sampleRate: number;
  languages: string[];
  vendors: SttVendor[];
  samples: { name: string; durationSec: number; hasReference: boolean }[];
}

interface Scored {
  mixed?: ErrorRate;
  cerZh?: ErrorRate;
  werEn?: ErrorRate;
}

function score(reference: string | null, pass: PassResult): Scored {
  if (!reference || !pass.ok || !pass.hypothesis) return {};
  return {
    mixed: mixedErrorRate(reference, pass.hypothesis),
    cerZh: cerChineseOnly(reference, pass.hypothesis),
    werEn: werEnglishOnly(reference, pass.hypothesis),
  };
}

const pct = (r: ErrorRate | undefined): string =>
  r === undefined ? '—' : `${(r.rate * 100).toFixed(2)}%`;
const ms = (n: number): string => `${Math.round(n)} ms`;
const usd = (n: number): string => `$${n.toFixed(4)}`;
const usdHr = (n: number): string => (n === 0 ? 'local' : `$${n.toFixed(2)}/hr`);

function mean(values: number[]): number | undefined {
  if (values.length === 0) return undefined;
  return values.reduce((a, b) => a + b, 0) / values.length;
}
const meanPct = (values: number[]): string => {
  const m = mean(values);
  return m === undefined ? '—' : `${(m * 100).toFixed(2)}%`;
};

export function renderMarkdown(runs: SampleRun[], meta: ReportMeta): string {
  const lines: string[] = [];
  lines.push(`# STT benchmark — ${meta.date}`);
  lines.push('');
  lines.push(
    `Corpus: \`${meta.dataDir}\` · ${meta.samples.length} sample(s) · ` +
      `sample rate ${meta.sampleRate} Hz · languages ${meta.languages.join(', ')}`,
  );
  lines.push('');

  // Samples.
  lines.push('## Samples');
  lines.push('');
  lines.push('| Sample | Duration | Reference |');
  lines.push('| --- | --- | --- |');
  for (const s of meta.samples) {
    const dur = s.durationSec > 0 ? `${s.durationSec.toFixed(1)} s` : '—';
    lines.push(`| ${s.name} | ${dur} | ${s.hasReference ? 'yes' : 'MISSING'} |`);
  }
  lines.push('');

  // Overall per vendor.
  lines.push('## Overall (averaged across samples)');
  lines.push('');
  lines.push(
    '| Vendor | Batch mixed ER | Batch CER (zh) | Batch WER (en) | ' +
      'Live mixed ER | Live lat p50 | Live lat p95 | Batch cost | Live cost | Simplified? |',
  );
  lines.push('| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |');

  for (const vendor of meta.vendors) {
    const vruns = runs.filter((r) => r.vendor === vendor);
    const batchMixed: number[] = [];
    const batchCer: number[] = [];
    const batchWer: number[] = [];
    const liveMixed: number[] = [];
    const allLatencies: number[] = [];
    let simplified = false;

    for (const run of vruns) {
      const b = score(run.reference, run.batch);
      if (b.mixed) batchMixed.push(b.mixed.rate);
      if (b.cerZh) batchCer.push(b.cerZh.rate);
      if (b.werEn) batchWer.push(b.werEn.rate);
      const s = score(run.reference, run.stream);
      if (s.mixed) liveMixed.push(s.mixed.rate);
      allLatencies.push(...run.stream.latenciesMs);
      if (run.batch.rawContainedSimplified || run.stream.rawContainedSimplified) simplified = true;
    }

    const price = PRICING[vendor];
    lines.push(
      `| ${vendor} | ${meanPct(batchMixed)} | ${meanPct(batchCer)} | ${meanPct(batchWer)} | ` +
        `${meanPct(liveMixed)} | ${allLatencies.length ? ms(percentile(allLatencies, 50)) : '—'} | ` +
        `${allLatencies.length ? ms(percentile(allLatencies, 95)) : '—'} | ` +
        `${usdHr(price.batchUsdPerHour)} | ${usdHr(price.liveUsdPerHour)} | ${simplified ? 'YES ⚠️' : 'no'} |`,
    );
  }
  lines.push('');

  // Per-sample detail.
  lines.push('## Per-sample detail');
  lines.push('');
  for (const run of runs) {
    const b = score(run.reference, run.batch);
    const s = score(run.reference, run.stream);
    const header = `### ${run.vendor} · ${run.sample}`;
    lines.push(header);
    lines.push('');
    if (!run.reference) lines.push('> No reference transcript — not scored.');
    lines.push('');
    lines.push(
      '| Pass | Mixed ER | CER (zh) | WER (en) | Latency p50 | Latency p95 | Est. cost | Status |',
    );
    lines.push('| --- | --- | --- | --- | --- | --- | --- | --- |');
    const batchCost =
      run.durationSec > 0
        ? usd(estimateCost(PRICING[run.vendor].batchUsdPerHour, run.durationSec))
        : '—';
    lines.push(
      `| batch | ${pct(b.mixed)} | ${pct(b.cerZh)} | ${pct(b.werEn)} | — | — | ${batchCost} | ${passStatus(run.batch)} |`,
    );
    const p50 = run.stream.latenciesMs.length ? ms(percentile(run.stream.latenciesMs, 50)) : '—';
    const p95 = run.stream.latenciesMs.length ? ms(percentile(run.stream.latenciesMs, 95)) : '—';
    const liveCost =
      run.durationSec > 0
        ? usd(estimateCost(PRICING[run.vendor].liveUsdPerHour, run.durationSec))
        : '—';
    lines.push(
      `| live | ${pct(s.mixed)} | ${pct(s.cerZh)} | ${pct(s.werEn)} | ${p50} | ${p95} | ${liveCost} | ${passStatus(run.stream)} |`,
    );
    lines.push('');
  }

  // Pricing sources.
  lines.push('## Pricing sources');
  lines.push('');
  for (const vendor of meta.vendors) {
    lines.push(`- **${vendor}**: ${PRICING[vendor].source}`);
  }
  lines.push('');
  lines.push(
    '> Error rates are edit distance over the reference. "Simplified?" flags whether the ' +
      "vendor's raw output contained Simplified characters before conversion to Traditional " +
      "(Taiwan). Latency is wall-clock from a segment's audio end to its arrival, feeding at 1×.",
  );
  lines.push('');
  return lines.join('\n');
}

function passStatus(pass: PassResult | StreamPassResult): string {
  if (pass.ok) return 'ok';
  return pass.error ? `error: ${pass.error}` : 'error';
}

export function renderCsv(runs: SampleRun[]): string {
  const header = [
    'vendor',
    'sample',
    'mode',
    'reference_chars',
    'mixed_er',
    'cer_zh',
    'wer_en',
    'latency_p50_ms',
    'latency_p95_ms',
    'duration_sec',
    'est_cost_usd',
    'raw_simplified',
    'ok',
    'error',
  ];
  const rows: string[] = [header.join(',')];

  const emit = (
    run: SampleRun,
    mode: 'batch' | 'live',
    pass: PassResult | StreamPassResult,
  ): void => {
    const sc = score(run.reference, pass);
    const latencies = 'latenciesMs' in pass ? pass.latenciesMs : [];
    const rate =
      mode === 'batch' ? PRICING[run.vendor].batchUsdPerHour : PRICING[run.vendor].liveUsdPerHour;
    const cost = run.durationSec > 0 ? estimateCost(rate, run.durationSec) : 0;
    const cell = (v: string | number): string => {
      const s = String(v);
      return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
    };
    rows.push(
      [
        run.vendor,
        run.sample,
        mode,
        run.reference ? run.reference.length : 0,
        sc.mixed ? sc.mixed.rate.toFixed(4) : '',
        sc.cerZh ? sc.cerZh.rate.toFixed(4) : '',
        sc.werEn ? sc.werEn.rate.toFixed(4) : '',
        latencies.length ? Math.round(percentile(latencies, 50)) : '',
        latencies.length ? Math.round(percentile(latencies, 95)) : '',
        run.durationSec ? run.durationSec.toFixed(1) : '',
        cost ? cost.toFixed(4) : '',
        pass.rawContainedSimplified ? 'yes' : 'no',
        pass.ok ? 'yes' : 'no',
        pass.error ?? '',
      ]
        .map(cell)
        .join(','),
    );
  };

  for (const run of runs) {
    emit(run, 'batch', run.batch);
    emit(run, 'live', run.stream);
  }
  return rows.join('\n');
}
