# ===== BUILD STAGE ======
FROM rust:1.88-bookworm AS builder

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    clang \
    cmake \
    git \
    pkg-config \
    libssl-dev \
    protobuf-compiler \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /gear
COPY . .

RUN cargo build -p ethexe-cli -p ethexe-node-loader --release

# ===== RUNTIME STAGE ======
FROM debian:12-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /gear/target/release/ethexe /usr/local/bin/ethexe
COPY --from=builder /gear/target/release/ethexe-node-loader /usr/local/bin/ethexe-node-loader

CMD ["ethexe", "--help"]
