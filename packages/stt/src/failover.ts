import type { Track } from '@hearme/shared';
import type {
  SttAdapter,
  SttBatchOptions,
  SttStream,
  SttStreamHandlers,
  SttStreamOptions,
} from './adapter.js';

/**
 * Wraps a primary and a fallback adapter so that if the primary fails — either
 * when opening the stream or mid-meeting — the session transparently switches to
 * the fallback vendor and keeps going. Only if the fallback also fails does the
 * error reach the caller.
 *
 * Audio already relayed to the primary is gone (vendors keep no replay buffer), so
 * failover continues from the next chunk; the final batch pass fills any gap. The
 * client-side resume window (reconnecting with the same meetingId) is a separate
 * mechanism and is unaffected.
 */
export class FailoverAdapter implements SttAdapter {
  readonly name: string;
  readonly supportsCodeSwitching: boolean;

  constructor(
    private readonly primary: SttAdapter,
    private readonly fallback: SttAdapter,
    private readonly onFailover?: (from: string, to: string, error: Error) => void,
  ) {
    this.name = `${primary.name}->${fallback.name}`;
    this.supportsCodeSwitching = primary.supportsCodeSwitching && fallback.supportsCodeSwitching;
  }

  async openStream(options: SttStreamOptions, handlers: SttStreamHandlers): Promise<SttStream> {
    let active: SttStream | undefined;
    let switching = false;
    let switched = false;
    // Chunks that arrive while we are opening the fallback stream, replayed once ready.
    const pending: { track: Track; chunk: Uint8Array; timestampMs: number }[] = [];

    const switchToFallback = (error: Error): void => {
      if (switched || switching) return;
      switching = true;
      this.onFailover?.(this.primary.name, this.fallback.name, error);
      this.fallback
        .openStream(options, {
          onSegment: handlers.onSegment,
          // The fallback is the last line of defence: its errors are real.
          onError: handlers.onError,
        })
        .then((stream) => {
          active = stream;
          switched = true;
          switching = false;
          for (const p of pending.splice(0)) stream.pushAudio(p.track, p.chunk, p.timestampMs);
        })
        .catch((err: unknown) => {
          switching = false;
          handlers.onError(err instanceof Error ? err : new Error(String(err)));
        });
    };

    const primaryHandlers: SttStreamHandlers = {
      onSegment: handlers.onSegment,
      onError: (error) => switchToFallback(error),
    };

    try {
      active = await this.primary.openStream(options, primaryHandlers);
    } catch (error) {
      // Primary failed to open at all: go straight to the fallback.
      active = await this.fallback.openStream(options, handlers);
      switched = true;
      this.onFailover?.(
        this.primary.name,
        this.fallback.name,
        error instanceof Error ? error : new Error(String(error)),
      );
    }

    return {
      pushAudio: (track, chunk, timestampMs) => {
        if (switching || !active) {
          pending.push({ track, chunk, timestampMs });
          return;
        }
        active.pushAudio(track, chunk, timestampMs);
      },
      close: async () => {
        if (active) await active.close();
      },
    };
  }

  async transcribeBatch(options: SttBatchOptions) {
    try {
      return await this.primary.transcribeBatch(options);
    } catch (error) {
      this.onFailover?.(
        this.primary.name,
        this.fallback.name,
        error instanceof Error ? error : new Error(String(error)),
      );
      return this.fallback.transcribeBatch(options);
    }
  }
}
