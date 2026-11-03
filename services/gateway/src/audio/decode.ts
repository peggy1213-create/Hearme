import OpusScript from 'opusscript';
import type { Codec } from '@hearme/shared';

/**
 * Decodes incoming audio frames to raw 16-bit mono PCM for the STT adapters.
 * `opus` frames are decoded with opusscript (a pure-JS codec, so no native build);
 * `pcm` frames pass straight through. A decoder is stateful, so each track gets
 * its own instance.
 */
export interface AudioDecoder {
  decode(frame: Uint8Array): Uint8Array;
  close(): void;
}

class OpusDecoder implements AudioDecoder {
  private readonly codec: InstanceType<typeof OpusScript>;

  constructor(sampleRate: number) {
    // opusscript types the rate as a literal union; 16 kHz (our target) is valid.
    const rate = sampleRate as 8000 | 12000 | 16000 | 24000 | 48000;
    this.codec = new OpusScript(rate, 1, OpusScript.Application.VOIP);
  }

  decode(frame: Uint8Array): Uint8Array {
    const out = this.codec.decode(Buffer.from(frame));
    return new Uint8Array(out.buffer, out.byteOffset, out.byteLength);
  }

  close(): void {
    this.codec.delete();
  }
}

class PassthroughDecoder implements AudioDecoder {
  decode(frame: Uint8Array): Uint8Array {
    return frame;
  }
  close(): void {}
}

/** Create a decoder for the session's codec and sample rate. */
export function createDecoder(codec: Codec, sampleRate: number): AudioDecoder {
  return codec === 'opus' ? new OpusDecoder(sampleRate) : new PassthroughDecoder();
}
