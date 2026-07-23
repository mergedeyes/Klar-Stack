# --- Dependencies stage ---
FROM node:20-slim AS deps
WORKDIR /app
COPY package.json package-lock.json ./
RUN npm ci

# --- Build stage ---
FROM node:20-slim AS builder
WORKDIR /app
COPY --from=deps /app/node_modules ./node_modules
COPY . .

# Public env vars must be present at BUILD time — Next.js inlines
# NEXT_PUBLIC_* values into the client bundle during `next build`.
# Setting these as runtime env vars on Bunny (or anywhere else the
# already-built container just runs) has zero effect -- the build
# happens here, in GitHub Actions, before the image is ever pushed.
ARG NEXT_PUBLIC_API_URL
ARG NEXT_PUBLIC_STORAGE_URL
ARG NEXT_PUBLIC_ADMIN_USER_ID
ENV NEXT_PUBLIC_API_URL=$NEXT_PUBLIC_API_URL
ENV NEXT_PUBLIC_STORAGE_URL=$NEXT_PUBLIC_STORAGE_URL
ENV NEXT_PUBLIC_ADMIN_USER_ID=$NEXT_PUBLIC_ADMIN_USER_ID

RUN npm run build

# --- Runtime stage ---
FROM node:20-slim AS runtime
WORKDIR /app
ENV NODE_ENV=production

# Standalone output: a self-contained server.js plus a trimmed node_modules,
# instead of the full dependency tree
COPY --from=builder /app/.next/standalone ./
COPY --from=builder /app/.next/static ./.next/static
COPY --from=builder /app/public ./public

RUN useradd --system --create-home --shell /usr/sbin/nologin appuser \
    # Next's image optimizer writes to .next/cache at runtime — everything
    # copied above is root-owned by default, so without this, appuser gets
    # EACCES trying to mkdir it the first time an image is requested.
    && mkdir -p .next/cache \
    && chown -R appuser:appuser /app

USER appuser

ENV PORT=3000
EXPOSE 3000

CMD ["node", "server.js"]
