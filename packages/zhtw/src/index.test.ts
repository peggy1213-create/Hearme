import { describe, expect, it } from 'vitest';
import { toTaiwan } from './index.js';

describe('toTaiwan', () => {
  it('converts Simplified to Traditional with Taiwan wording', () => {
    expect(toTaiwan('软件')).toBe('軟體');
    expect(toTaiwan('视频')).toBe('影片');
  });

  it('leaves already-Traditional Taiwan text unchanged', () => {
    expect(toTaiwan('軟體')).toBe('軟體');
  });

  it('leaves English and mixed text intact (only Chinese is converted)', () => {
    expect(toTaiwan('let us open the 软件')).toBe('let us open the 軟體');
    expect(toTaiwan('hello world')).toBe('hello world');
  });

  it('handles empty input', () => {
    expect(toTaiwan('')).toBe('');
  });
});
