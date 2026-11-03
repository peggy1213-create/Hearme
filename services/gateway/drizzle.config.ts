import { defineConfig } from 'drizzle-kit';

/**
 * drizzle-kit config for generating migrations from `src/db/schema.ts`.
 * Runtime migration (dev + prod) goes through `src/db/migrate.ts`; this file is
 * only for `pnpm db:generate`.
 */
export default defineConfig({
  dialect: 'postgresql',
  schema: './src/db/schema.ts',
  out: './drizzle',
  dbCredentials: {
    url: process.env.DATABASE_URL ?? 'postgresql://hearme:hearme@localhost:5432/hearme',
  },
});
