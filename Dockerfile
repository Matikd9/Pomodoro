# ==========================================
# Stage 1: Build Frontend SPA
# ==========================================
FROM node:22-alpine AS frontend-builder
WORKDIR /app

COPY package*.json ./
RUN npm ci

COPY . .
RUN npm run build

# ==========================================
# Stage 2: Build Headless Rust Server
# ==========================================
FROM rust:alpine AS server-builder
RUN apk add --no-cache musl-dev

WORKDIR /app
COPY src-tauri ./src-tauri
COPY static ./static
COPY server ./server

WORKDIR /app/server
RUN cargo build --release

# ==========================================
# Stage 3: Minimal Runtime
# ==========================================
FROM alpine:3.20
RUN apk add --no-cache ca-certificates tzdata

ENV PORT=8085 \
    DATA_DIR=/data \
    STATIC_DIR=/app/dist \
    RUST_LOG=info

WORKDIR /app

# Copy binary and frontend assets
COPY --from=server-builder /app/server/target/release/pomotroid-server /usr/local/bin/pomotroid-server
COPY --from=frontend-builder /app/build /app/dist

# Create persistent data directory
RUN mkdir -p /data

EXPOSE 8085

VOLUME ["/data"]

CMD ["/usr/local/bin/pomotroid-server"]
