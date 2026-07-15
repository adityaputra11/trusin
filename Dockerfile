FROM rust:1.88-alpine AS builder
RUN apk add --no-cache musl-dev pkgconfig openssl-dev
WORKDIR /app
COPY Cargo.toml Cargo.lock* ./
COPY apps ./apps
RUN cargo build --release --bin backend --bin web

FROM alpine:3.21
RUN apk add --no-cache ca-certificates
COPY --from=builder /app/target/release/backend /usr/local/bin/
COPY --from=builder /app/target/release/web /usr/local/bin/
COPY apps/backend/migrations /migrations
CMD ["backend"]
