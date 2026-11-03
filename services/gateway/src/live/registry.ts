import type { LiveSession } from '../session.js';

interface RegistryEntry {
  session: LiveSession;
  /** Id of the connection currently bound to the session. */
  activeConnId: number;
  resumeTimer?: ReturnType<typeof setTimeout>;
}

/**
 * Tracks live sessions by meetingId so a dropped socket can reconnect and resume.
 * A session whose socket drops is kept for `resumeWindowMs`; if no reconnect
 * arrives it is aborted and removed. Everything here is in memory only.
 */
export class SessionRegistry {
  private readonly entries = new Map<string, RegistryEntry>();

  get(meetingId: string): LiveSession | undefined {
    return this.entries.get(meetingId)?.session;
  }

  /** Register (or re-register, on resume) a session for a connection. */
  bind(meetingId: string, session: LiveSession, connId: number): void {
    const existing = this.entries.get(meetingId);
    if (existing?.resumeTimer) clearTimeout(existing.resumeTimer);
    this.entries.set(meetingId, { session, activeConnId: connId });
  }

  /** Is `connId` still the live connection for this meeting? */
  isActiveConn(meetingId: string, connId: number): boolean {
    return this.entries.get(meetingId)?.activeConnId === connId;
  }

  /** Remove a session immediately (e.g. after `stop`). */
  remove(meetingId: string): void {
    const entry = this.entries.get(meetingId);
    if (entry?.resumeTimer) clearTimeout(entry.resumeTimer);
    this.entries.delete(meetingId);
  }

  /**
   * The socket for `connId` dropped: keep the session resumable for the window,
   * then abort it if nobody reconnected.
   */
  scheduleAbort(meetingId: string, connId: number, resumeWindowMs: number): void {
    const entry = this.entries.get(meetingId);
    if (!entry || entry.activeConnId !== connId) return; // a newer connection owns it
    entry.resumeTimer = setTimeout(() => {
      const current = this.entries.get(meetingId);
      if (current && current.activeConnId === connId) {
        void current.session.abort();
        this.entries.delete(meetingId);
      }
    }, resumeWindowMs);
    // Do not keep the process alive just for the resume window.
    entry.resumeTimer.unref?.();
  }

  get size(): number {
    return this.entries.size;
  }

  /** Abort every session (server shutdown). */
  async closeAll(): Promise<void> {
    const sessions = [...this.entries.values()];
    this.entries.clear();
    for (const e of sessions) {
      if (e.resumeTimer) clearTimeout(e.resumeTimer);
      await e.session.abort();
    }
  }
}
