import { fileURLToPath } from 'node:url';
import type { ExtractTablesWithRelations } from 'drizzle-orm';
import type { PgDatabase, PgQueryResultHKT } from 'drizzle-orm/pg-core';
import { schema } from './schema.js';

/**
 * Database access for the gateway. Production uses PostgreSQL (the `postgres`
 * driver) via `DATABASE_URL`. Tests use PGlite — an in-process PostgreSQL — so the
 * whole suite runs with no Docker and no live database. Both use the same Drizzle
 * schema and the same migrations folder.
 */

const MIGRATIONS_FOLDER = fileURLToPath(new URL('../../drizzle', import.meta.url));

/** A Drizzle handle usable with either driver. */
export type GatewayDatabase = PgDatabase<
  PgQueryResultHKT,
  typeof schema,
  ExtractTablesWithRelations<typeof schema>
>;

export interface GatewayDb {
  db: GatewayDatabase;
  /** Apply all pending migrations. */
  migrate: () => Promise<void>;
  /** Close the underlying connection/engine. */
  close: () => Promise<void>;
}

export interface CreateDbOptions {
  /** Connection string. `memory://` (or unset in tests) selects in-process PGlite. */
  url?: string;
}

function isMemoryUrl(url: string | undefined): boolean {
  return !url || url.startsWith('memory:') || url.startsWith('pglite:');
}

/** Open a database handle, choosing the driver from the URL. */
export async function createDb(options: CreateDbOptions = {}): Promise<GatewayDb> {
  const url = options.url ?? process.env.DATABASE_URL;

  if (isMemoryUrl(url)) {
    const { PGlite } = await import('@electric-sql/pglite');
    const { drizzle } = await import('drizzle-orm/pglite');
    const { migrate } = await import('drizzle-orm/pglite/migrator');
    const client = new PGlite(); // in-memory
    const db = drizzle(client, { schema });
    return {
      db: db as unknown as GatewayDatabase,
      migrate: () => migrate(db, { migrationsFolder: MIGRATIONS_FOLDER }),
      close: () => client.close(),
    };
  }

  const { default: postgres } = await import('postgres');
  const { drizzle } = await import('drizzle-orm/postgres-js');
  const { migrate } = await import('drizzle-orm/postgres-js/migrator');
  const client = postgres(url!, { max: 10 });
  const db = drizzle(client, { schema });
  return {
    db: db as unknown as GatewayDatabase,
    migrate: () => migrate(db, { migrationsFolder: MIGRATIONS_FOLDER }),
    close: () => client.end(),
  };
}
