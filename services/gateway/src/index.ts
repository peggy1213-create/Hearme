import { buildServer } from './server.js';

const port = Number(process.env.PORT ?? 8787);
const host = process.env.HOST ?? '0.0.0.0';

const app = buildServer();

app
  .listen({ port, host })
  .then((address) => {
    app.log.info(`hearme-gateway listening on ${address}`);
  })
  .catch((err: unknown) => {
    app.log.error(err);
    process.exit(1);
  });
