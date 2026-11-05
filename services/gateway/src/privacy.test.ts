import { afterEach, describe, expect, it } from 'vitest';
import { sql } from 'drizzle-orm';
import { usage, users } from './db/schema.js';
import { FakeClient, startGateway, type TestGateway } from './testutil/gateway.js';

/** Normalise an `execute` result across the postgres-js and PGlite drivers. */
function toRows(result: unknown): Record<string, unknown>[] {
  if (Array.isArray(result)) return result as Record<string, unknown>[];
  const maybe = (result as { rows?: unknown }).rows;
  return Array.isArray(maybe) ? (maybe as Record<string, unknown>[]) : [];
}

// Transcript fragments the mock vendor produces; none may ever be persisted/logged.
const TRANSCRIPT_FRAGMENTS = ['大家好', '這個', 'sprint', 'sounds good to me', '我同意'];

describe('no meeting content on the server', () => {
  let tg: TestGateway;
  afterEach(async () => {
    await tg.close();
  });

  it('after a replay, logs and the database hold no transcript text', async () => {
    tg = await startGateway();

    const c = await FakeClient.connect(tg.liveUrl);
    c.start('m-privacy');
    await c.waitUntil(() => c.acks.length >= 2);
    for (let seq = 0; seq < 10; seq++) c.audio('meeting', seq);
    await c.waitUntil(() => c.segments.length >= 4);
    c.stop('m-privacy');
    await c.waitUntil(() => c.ended !== undefined);
    c.close();

    // Transcription really happened...
    expect(c.segments.length).toBeGreaterThan(0);

    // ...but none of the text appears in the logs.
    for (const fragment of TRANSCRIPT_FRAGMENTS) {
      expect(tg.logs).not.toContain(fragment);
    }

    // The database holds ONLY users and usage tables (public schema).
    const tables = toRows(
      await tg.db.db.execute(
        sql`select table_name from information_schema.tables where table_schema = 'public' order by table_name`,
      ),
    ).map((r) => String(r.table_name));
    expect(tables).toEqual(['usage', 'users']);

    // The rows that do exist carry no transcript text.
    const userRows = await tg.db.db.select().from(users);
    const usageRows = await tg.db.db.select().from(usage);
    expect(userRows).toHaveLength(1);
    expect(usageRows).toHaveLength(1);
    expect(usageRows[0]!.liveMinutes).toBeGreaterThan(0);

    const dump = JSON.stringify([...userRows, ...usageRows]);
    for (const fragment of TRANSCRIPT_FRAGMENTS) {
      expect(dump).not.toContain(fragment);
    }
  });
});
