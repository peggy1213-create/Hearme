import { createDb } from './db/client.js';
import { buildGateway } from './server.js';
import { startTelemetry } from './otel.js';

const port = Number(process.env.PORT ?? 8787);
const host = process.env.HOST ?? '0.0.0.0';

async function main(): Promise<void> {
  const shutdownOtel = await startTelemetry();

  // Own the database and apply migrations before serving.
  const dbHandle = await createDb();
  await dbHandle.migrate();

  const gateway = await buildGateway({ db: dbHandle });
  const address = await gateway.app.listen({ port, host });
  gateway.app.log.info(`hearme-gateway listening on ${address}`);

  const stop = async (): Promise<void> => {
    await gateway.close();
    await dbHandle.close();
    await shutdownOtel?.();
    process.exit(0);
  };
  process.on('SIGTERM', () => void stop());
  process.on('SIGINT', () => void stop());
}

main().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});
