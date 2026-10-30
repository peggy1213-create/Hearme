import { toTaiwanTraditional, tokenizeMixed } from '@hearme/zhtw';

/**
 * Accuracy metrics for the Phase 0 STT benchmark.
 *
 * - CER (character error rate) for Chinese: Chinese has no spaces, so we score per
 *   character.
 * - WER (word error rate) for English: scored per whitespace-delimited word.
 * - Mixed error rate: over `tokenizeMixed` tokens (Han chars + English words), so
 *   code-switched speech is scored as one sequence.
 *
 * All are edit distance (insertions + deletions + substitutions) over the
 * reference length. Chinese is converted to Traditional (Taiwan) on both sides
 * first, so Simplified vs Traditional is never counted as an error. See
 * docs/plan.md.
 */

const HAN = /\p{Script=Han}/u;

export interface ErrorRate {
  /** errors / reference length, in [0, ∞). 0 is perfect. */
  rate: number;
  insertions: number;
  deletions: number;
  substitutions: number;
  referenceLength: number;
}

/** Levenshtein distance over two token arrays, with an operation breakdown. */
export function editOps(ref: readonly string[], hyp: readonly string[]): ErrorRate {
  const n = ref.length;
  const m = hyp.length;

  // dp[i][j] = min edits to turn ref[0..i) into hyp[0..j). Track op counts too.
  type Cell = { cost: number; ins: number; del: number; sub: number };
  const dp: Cell[][] = Array.from({ length: n + 1 }, () =>
    Array.from({ length: m + 1 }, () => ({ cost: 0, ins: 0, del: 0, sub: 0 })),
  );

  for (let j = 1; j <= m; j++) {
    const prev = dp[0]![j - 1]!;
    dp[0]![j] = { cost: j, ins: prev.ins + 1, del: prev.del, sub: prev.sub };
  }
  for (let i = 1; i <= n; i++) {
    const prev = dp[i - 1]![0]!;
    dp[i]![0] = { cost: i, ins: prev.ins, del: prev.del + 1, sub: prev.sub };
  }

  for (let i = 1; i <= n; i++) {
    for (let j = 1; j <= m; j++) {
      if (ref[i - 1] === hyp[j - 1]) {
        dp[i]![j] = { ...dp[i - 1]![j - 1]! };
        continue;
      }
      const sub = dp[i - 1]![j - 1]!;
      const del = dp[i - 1]![j]!;
      const ins = dp[i]![j - 1]!;
      const best = Math.min(sub.cost, del.cost, ins.cost);
      if (best === sub.cost) {
        dp[i]![j] = { cost: sub.cost + 1, ins: sub.ins, del: sub.del, sub: sub.sub + 1 };
      } else if (best === del.cost) {
        dp[i]![j] = { cost: del.cost + 1, ins: del.ins, del: del.del + 1, sub: del.sub };
      } else {
        dp[i]![j] = { cost: ins.cost + 1, ins: ins.ins + 1, del: ins.del, sub: ins.sub };
      }
    }
  }

  const final = dp[n]![m]!;
  return {
    rate: n === 0 ? (m === 0 ? 0 : 1) : final.cost / n,
    insertions: final.ins,
    deletions: final.del,
    substitutions: final.sub,
    referenceLength: n,
  };
}

/** Tokenise English into lowercased words, stripping punctuation. */
export function wordTokens(text: string): string[] {
  return text
    .toLowerCase()
    .replace(/[^\p{L}\p{N}\s']/gu, ' ')
    .split(/\s+/)
    .filter(Boolean);
}

/** Split into characters (code points), dropping whitespace. For Chinese CER. */
export function charTokens(text: string): string[] {
  return [...text].filter((ch) => !/\s/u.test(ch));
}

/** Word error rate for English. */
export function wer(reference: string, hypothesis: string): ErrorRate {
  return editOps(wordTokens(reference), wordTokens(hypothesis));
}

/**
 * Character error rate for Chinese. Both sides are normalised to Traditional
 * (Taiwan) first, so Simplified vs Traditional is never counted as an error.
 */
export function cer(reference: string, hypothesis: string): ErrorRate {
  return editOps(
    charTokens(toTaiwanTraditional(reference)),
    charTokens(toTaiwanTraditional(hypothesis)),
  );
}

/**
 * Mixed error rate over `tokenizeMixed` tokens (one token per Han character, one
 * per English word). This is the headline metric for code-switched zh-TW/English
 * speech. Both sides are converted to Traditional (Taiwan) first.
 */
export function mixedErrorRate(reference: string, hypothesis: string): ErrorRate {
  return editOps(
    tokenizeMixed(toTaiwanTraditional(reference)),
    tokenizeMixed(toTaiwanTraditional(hypothesis)),
  );
}

/** Keep only the Han-character tokens from a mixed string (after conversion). */
function chineseChars(text: string): string[] {
  return tokenizeMixed(toTaiwanTraditional(text)).filter((t) => HAN.test(t));
}

/** Keep only the non-Han (English/number) word tokens from a mixed string. */
function englishWords(text: string): string[] {
  return tokenizeMixed(text).filter((t) => !HAN.test(t));
}

/** CER over the Chinese (Han) characters only, ignoring the English parts. */
export function cerChineseOnly(reference: string, hypothesis: string): ErrorRate {
  return editOps(chineseChars(reference), chineseChars(hypothesis));
}

/** WER over the English words only, ignoring the Chinese parts. */
export function werEnglishOnly(reference: string, hypothesis: string): ErrorRate {
  return editOps(englishWords(reference), englishWords(hypothesis));
}

/**
 * The p-th percentile (0–100) of a set of numbers, using nearest-rank. Returns 0
 * for an empty set. Used for streaming latency p50/p95.
 */
export function percentile(values: readonly number[], p: number): number {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const rank = Math.ceil((p / 100) * sorted.length);
  const idx = Math.min(sorted.length - 1, Math.max(0, rank - 1));
  return sorted[idx]!;
}
