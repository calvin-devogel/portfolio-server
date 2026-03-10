# build
FROM lukemathwalker/cargo-chef:latest-rust-1.93-alpine3.23 AS chef
WORKDIR /app

ARG TARGET_ARCH=x86_64
ENV TARGET=${TARGET_ARCH}

RUN rustup target add ${TARGET}-unknown-linux-musl
RUN apk update && apk add musl-utils musl-dev
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json

COPY . .
ENV SQLX_OFFLINE=true
RUN cargo build --target=${TARGET}-unknown-linux-musl --release --bin portfolio-server

FROM alpine:latest AS runtime
ARG TARGET_ARCH=x86_64
ENV TARGET=${TARGET_ARCH}
WORKDIR /app
RUN apk update && apk add openssl ca-certificates
COPY --from=builder /app/target/${TARGET}-unknown-linux-musl/release/portfolio-server ./
COPY configuration configuration
ENV APP_ENVIRONMENT=production
ENTRYPOINT ["./portfolio-server"]
