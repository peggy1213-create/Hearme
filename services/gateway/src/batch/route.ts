import { randomUUID } from 'node:crypto';
import { unlink, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { FastifyInstance } from 'fastify';
import { z } from 'zod';
import { LangSchema } from '@hearme/shared';
import type { GatewayContext } from '../context.js';
import { resolveUserId } from '../auth/authenticate.js';
import { addUsage, checkQuota, getUser } from '../db/store.js';
import { ensureTraditional } from '../zhtwGuard.js';

const QuerySchema = z.object({
  meetingId: z.string().min(1),
  languages: z
    .string()
    .optional()
    .transform((s) => (s ? s.split(',') : undefined))
    .pipe(z.array(LangSchema).optional()),
});

/**
 * `POST /v1/batch` — the final (batch) transcription pass. The app uploads the
 * full recording as the raw request body; the gateway relays it to the batch STT
 * vendor and returns `pass: "final"` segments. Nothing is persisted: the upload is
 * streamed to a temp file only because the vendor batch API is path-based, and the
 * file is deleted immediately in `finally`. See PROTOCOL.md and CLAUDE.md.
 */
export function registerBatchRoute(app: FastifyInstance, ctx: GatewayContext): void {
  // Receive the raw audio body as a Buffer (kept in memory, never parsed/logged).
  app.addContentTypeParser('application/octet-stream', { parseAs: 'buffer' }, (_req, body, done) =>
    done(null, body),
  );

  app.post('/v1/batch', async (req, reply) => {
    let userId: string;
    try {
      userId = await resolveUserId(ctx, req.headers.authorization, undefined);
    } catch {
      return reply.code(401).send({ error: 'authentication required' });
    }

    const query = QuerySchema.safeParse(req.query);
    if (!query.success) return reply.code(400).send({ error: 'meetingId required' });

    const user = await getUser(ctx.db, userId);
    if (!user) return reply.code(401).send({ error: 'unknown user' });
    const quota = await checkQuota(ctx.db, user);
    if (!quota.allowed) return reply.code(429).send({ error: 'quota exceeded' });

    const tmpPath = join(tmpdir(), `hearme-batch-${randomUUID()}`);
    try {
      await writeFile(tmpPath, req.body as Buffer);

      const adapter = ctx.buildBatchAdapter();
      const segments = (
        await adapter.transcribeBatch({
          meetingId: query.data.meetingId,
          languages: query.data.languages ?? ['en', 'zh-TW'],
          audioPath: tmpPath,
          normalizeChinese: true,
        })
      ).map(ensureTraditional);

      const durationMs = segments.reduce((max, s) => Math.max(max, s.endMs), 0);
      await addUsage(ctx.db, userId, { finalMinutes: durationMs / 60_000 });

      return reply.send({ meetingId: query.data.meetingId, segments });
    } catch {
      return reply.code(502).send({ error: 'final pass failed' });
    } finally {
      await unlink(tmpPath).catch(() => {});
    }
  });
}
