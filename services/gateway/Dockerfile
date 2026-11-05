# Hearme gateway image. Built from the repo root so workspace packages resolve:
#   docker build -f services/gateway/Dockerfile -t hearme-gateway .
#
# The gateway stores no meeting content; only accounts/usage in PostgreSQL.

# --- build: install the whole workspace and compile TS -> dist ---
FROM node:22-slim AS build
RUN corepack enable
WORKDIR /app
COPY . .
RUN pnpm install --frozen-lockfile
RUN pnpm build

# --- runtime ---
FROM node:22-slim AS runtime
RUN corepack enable
WORKDIR /app
COPY --from=build /app /app
WORKDIR /app/services/gateway
ENV NODE_ENV=production
ENV PORT=8787
EXPOSE 8787

# Health check hits the /health endpoint using Node's built-in fetch.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD node -e "fetch('http://127.0.0.1:'+(process.env.PORT||8787)+'/health').then(r=>process.exit(r.ok?0:1)).catch(()=>process.exit(1))"

CMD ["node", "dist/index.js"]
