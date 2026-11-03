import { createDb } from './client.js';

/**
 * Apply pending migrations to the configured database (`DATABASE_URL`).
 * Run with `pnpm --filter @hearme/gateway db:migrate`.
 */
async function main(): Promise<void> {
  const handle = await createDb();
  try {
    await handle.migrate();
    console.log('migrations applied');
  } finally {
    await handle.close();
  }
}

main().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});
