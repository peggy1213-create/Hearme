import { describe, expect, it } from 'vitest';
import {
  containsSimplified,
  normaliseForScoring,
  toTaiwan,
  toTaiwanTraditional,
  tokenizeMixed,
} from './index.js';

describe('toTaiwanTraditional', () => {
  it('converts Simplified to Traditional with Taiwan wording', () => {
    expect(toTaiwanTraditional('软件')).toBe('軟體');
    expect(toTaiwanTraditional('视频')).toBe('影片');
  });

  it('leaves already-Traditional Taiwan text unchanged', () => {
    expect(toTaiwanTraditional('軟體')).toBe('軟體');
  });

  it('leaves English and mixed text intact (only Chinese is converted)', () => {
    expect(toTaiwanTraditional('let us open the 软件')).toBe('let us open the 軟體');
    expect(toTaiwanTraditional('hello world')).toBe('hello world');
  });

  it('handles empty input', () => {
    expect(toTaiwanTraditional('')).toBe('');
  });

  it('toTaiwan is an alias', () => {
    expect(toTaiwan).toBe(toTaiwanTraditional);
  });
});

describe('normaliseForScoring', () => {
  it('converts full-width Latin, digits and space to half-width', () => {
    expect(normaliseForScoring('ＨＥＬＬＯ　１２３')).toBe('hello 123');
  });

  it('removes punctuation and lowercases English', () => {
    expect(normaliseForScoring('Hello, World!')).toBe('hello world');
    expect(normaliseForScoring('會議，紀錄。')).toBe('會議 紀錄');
  });

  it('collapses whitespace and trims', () => {
    expect(normaliseForScoring('  a   b\t c ')).toBe('a b c');
  });

  it('handles empty input', () => {
    expect(normaliseForScoring('')).toBe('');
  });
});

describe('tokenizeMixed', () => {
  it('splits Han into single tokens and English into words', () => {
    expect(tokenizeMixed('這個 sprint 我們先 focus 在 Core Audio')).toEqual([
      '這',
      '個',
      'sprint',
      '我',
      '們',
      '先',
      'focus',
      '在',
      'core',
      'audio',
    ]);
  });

  it('keeps digits within a word token', () => {
    expect(tokenizeMixed('nova3 很快')).toEqual(['nova3', '很', '快']);
  });

  it('is empty for empty input', () => {
    expect(tokenizeMixed('')).toEqual([]);
  });
});

describe('containsSimplified', () => {
  it('detects Simplified characters', () => {
    expect(containsSimplified('这是软件')).toBe(true);
  });

  it('is false for Traditional-only text', () => {
    expect(containsSimplified('這是軟體')).toBe(false);
  });

  it('is false for English and punctuation', () => {
    expect(containsSimplified('hello, world 123')).toBe(false);
  });
});
