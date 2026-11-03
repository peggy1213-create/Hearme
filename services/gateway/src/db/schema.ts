import {
  bigint,
  doublePrecision,
  integer,
  pgTable,
  primaryKey,
  text,
  timestamp,
  uniqueIndex,
} from 'drizzle-orm/pg-core';

/**
 * Accounts and usage — the ONLY data the gateway stores. No recordings,
 * transcripts or summaries ever reach the database. See CLAUDE.md.
 */

/** A signed-in user. One row per (provider, providerSub). */
export const users = pgTable(
  'users',
  {
    id: text('id').primaryKey(),
    /** `google` | `microsoft` | `dev` (the development-only bypass). */
    provider: text('provider').notNull(),
    /** Stable subject id from the OAuth provider. */
    providerSub: text('provider_sub').notNull(),
    email: text('email'),
    displayName: text('display_name'),
    /** Monthly transcription-minute quota, enforced on session start. */
    monthlyQuotaMinutes: integer('monthly_quota_minutes').notNull().default(1200),
    createdAt: timestamp('created_at', { withTimezone: true }).notNull().defaultNow(),
  },
  (t) => [uniqueIndex('users_provider_sub_idx').on(t.provider, t.providerSub)],
);

/** Per-user, per-month usage counters. */
export const usage = pgTable(
  'usage',
  {
    userId: text('user_id')
      .notNull()
      .references(() => users.id, { onDelete: 'cascade' }),
    /** Calendar month as `YYYY-MM` (UTC). */
    yearMonth: text('year_month').notNull(),
    /** Minutes of live streaming transcription this month. */
    liveMinutes: doublePrecision('live_minutes').notNull().default(0),
    /** Minutes of final batch-pass transcription this month. */
    finalMinutes: doublePrecision('final_minutes').notNull().default(0),
    /** LLM tokens (summaries, action items) this month. */
    llmTokens: bigint('llm_tokens', { mode: 'number' }).notNull().default(0),
  },
  (t) => [primaryKey({ columns: [t.userId, t.yearMonth] })],
);

export const schema = { users, usage };
export type SchemaUser = typeof users.$inferSelect;
export type SchemaUsage = typeof usage.$inferSelect;
