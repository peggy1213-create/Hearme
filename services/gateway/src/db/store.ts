import { randomUUID } from 'node:crypto';
import { and, eq, sql } from 'drizzle-orm';
import type { GatewayDatabase } from './client.js';
import { usage, users, type SchemaUsage, type SchemaUser } from './schema.js';

/** Current calendar month as `YYYY-MM` in UTC. */
export function currentYearMonth(now: Date = new Date()): string {
  return `${now.getUTCFullYear()}-${String(now.getUTCMonth() + 1).padStart(2, '0')}`;
}

export interface UpsertUserInput {
  provider: string;
  providerSub: string;
  email?: string | null;
  displayName?: string | null;
  monthlyQuotaMinutes?: number;
}

/** Insert or update the user for (provider, providerSub), returning the row. */
export async function upsertUser(db: GatewayDatabase, input: UpsertUserInput): Promise<SchemaUser> {
  const rows = await db
    .insert(users)
    .values({
      id: randomUUID(),
      provider: input.provider,
      providerSub: input.providerSub,
      email: input.email ?? null,
      displayName: input.displayName ?? null,
      ...(input.monthlyQuotaMinutes !== undefined
        ? { monthlyQuotaMinutes: input.monthlyQuotaMinutes }
        : {}),
    })
    .onConflictDoUpdate({
      target: [users.provider, users.providerSub],
      set: { email: input.email ?? null, displayName: input.displayName ?? null },
    })
    .returning();
  return rows[0]!;
}

/** Fetch a user by id, or `undefined`. */
export async function getUser(db: GatewayDatabase, id: string): Promise<SchemaUser | undefined> {
  const rows = await db.select().from(users).where(eq(users.id, id)).limit(1);
  return rows[0];
}

/** This month's usage row for a user, or `undefined` if none yet. */
export async function getMonthlyUsage(
  db: GatewayDatabase,
  userId: string,
  yearMonth: string = currentYearMonth(),
): Promise<SchemaUsage | undefined> {
  const rows = await db
    .select()
    .from(usage)
    .where(and(eq(usage.userId, userId), eq(usage.yearMonth, yearMonth)))
    .limit(1);
  return rows[0];
}

export interface UsageDelta {
  liveMinutes?: number;
  finalMinutes?: number;
  llmTokens?: number;
}

/** Add usage to a user's month, creating the row if needed. */
export async function addUsage(
  db: GatewayDatabase,
  userId: string,
  delta: UsageDelta,
  yearMonth: string = currentYearMonth(),
): Promise<void> {
  const live = delta.liveMinutes ?? 0;
  const final = delta.finalMinutes ?? 0;
  const tokens = delta.llmTokens ?? 0;
  await db
    .insert(usage)
    .values({
      userId,
      yearMonth,
      liveMinutes: live,
      finalMinutes: final,
      llmTokens: tokens,
    })
    .onConflictDoUpdate({
      target: [usage.userId, usage.yearMonth],
      set: {
        liveMinutes: sql`${usage.liveMinutes} + ${live}`,
        finalMinutes: sql`${usage.finalMinutes} + ${final}`,
        llmTokens: sql`${usage.llmTokens} + ${tokens}`,
      },
    });
}

export interface QuotaStatus {
  allowed: boolean;
  usedMinutes: number;
  quotaMinutes: number;
}

/**
 * Whether the user may start another session this month: used live + final
 * minutes must be below their monthly quota.
 */
export async function checkQuota(
  db: GatewayDatabase,
  user: SchemaUser,
  yearMonth: string = currentYearMonth(),
): Promise<QuotaStatus> {
  const row = await getMonthlyUsage(db, user.id, yearMonth);
  const usedMinutes = (row?.liveMinutes ?? 0) + (row?.finalMinutes ?? 0);
  return {
    allowed: usedMinutes < user.monthlyQuotaMinutes,
    usedMinutes,
    quotaMinutes: user.monthlyQuotaMinutes,
  };
}
