import { describe, expect, it } from 'vitest';
import {
  cer,
  cerChineseOnly,
  charTokens,
  editOps,
  mixedErrorRate,
  percentile,
  wer,
  werEnglishOnly,
  wordTokens,
} from './metrics.js';

describe('wer', () => {
  it('is 0 for identical English', () => {
    expect(wer('let us begin the meeting', 'let us begin the meeting').rate).toBe(0);
  });

  it('counts a single substitution', () => {
    const r = wer('the cat sat', 'the dog sat');
    expect(r.substitutions).toBe(1);
    expect(r.rate).toBeCloseTo(1 / 3);
  });

  it('ignores case and punctuation', () => {
    expect(wer('Hello, world!', 'hello world').rate).toBe(0);
  });
});

describe('cer', () => {
  it('is 0 when Simplified matches Traditional after normalisation', () => {
    // 软件 normalises to 軟體, so comparing against 軟體 is a perfect match.
    expect(cer('軟體', '软件').rate).toBe(0);
  });

  it('counts a wrong character', () => {
    const r = cer('會議紀錄', '會議記錄');
    expect(r.referenceLength).toBe(4);
    expect(r.substitutions).toBe(1);
  });
});

describe('tokenisers', () => {
  it('splits words and drops empties', () => {
    expect(wordTokens('  a  b ')).toEqual(['a', 'b']);
  });

  it('splits characters and drops whitespace', () => {
    expect(charTokens('你 好')).toEqual(['你', '好']);
  });
});

describe('editOps edge cases', () => {
  it('empty vs empty is 0', () => {
    expect(editOps([], []).rate).toBe(0);
  });

  it('empty reference with hypothesis is full error', () => {
    expect(editOps([], ['a']).rate).toBe(1);
  });
});

describe('mixedErrorRate', () => {
  const ref = '這個 sprint 我們先 focus 在 Core Audio';

  it('is 0 for an identical code-switched sentence', () => {
    expect(mixedErrorRate(ref, ref).rate).toBe(0);
  });

  it('is 0 when only Simplified vs Traditional differs', () => {
    // 这个 → 這個 after conversion, so no error.
    expect(mixedErrorRate(ref, '这个 sprint 我们先 focus 在 Core Audio').rate).toBe(0);
  });

  it('counts an English substitution against the 10-token reference', () => {
    const r = mixedErrorRate(ref, '這個 sprint 我們先 focus 在 core video');
    expect(r.referenceLength).toBe(10);
    expect(r.substitutions).toBe(1);
  });
});

describe('cerChineseOnly / werEnglishOnly', () => {
  const ref = '我們用 Soniox 做 benchmark';

  it('CER scores only the Han characters', () => {
    // Reference Han chars are 我 們 用 做 (4); English is ignored.
    const r = cerChineseOnly(ref, '我用 Soniox 做 benchmark');
    expect(r.referenceLength).toBe(4);
    expect(r.deletions).toBe(1); // 們 dropped
  });

  it('WER scores only the English words', () => {
    const r = werEnglishOnly(ref, '我們用 soniox 做 test');
    expect(r.referenceLength).toBe(2); // soniox, benchmark
    expect(r.substitutions).toBe(1); // benchmark → test
  });
});

describe('percentile', () => {
  it('returns 0 for an empty set', () => {
    expect(percentile([], 95)).toBe(0);
  });

  it('computes nearest-rank p50 and p95', () => {
    const v = [10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
    expect(percentile(v, 50)).toBe(50);
    expect(percentile(v, 95)).toBe(100);
  });
});
