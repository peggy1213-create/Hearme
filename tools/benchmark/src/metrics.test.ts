import { describe, expect, it } from 'vitest';
import { cer, charTokens, editOps, wer, wordTokens } from './metrics.js';

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
