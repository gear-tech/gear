FROM ubuntu:24.04 AS builder
WORKDIR /
ENV PATH="${PATH}:/root/.cargo/bin"
RUN apt update && \
    apt install --no-install-recommends --yes \
        build-essential \
        ca-certificates \
        clang \
        curl \
        git \
        protobuf-compiler && \
    rm -rf /var/lib/apt/lists/* && \
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- --default-toolchain none -y && \
    git clone --depth 1 --recurse-submodules https://github.com/gear-tech/gear.git && \
    cd gear && \
    cargo clean && \
    cargo build --package ethexe-cli --release && \
    cp ./target/release/ethexe ethexe-bin && \
    rm -rf /root/.cargo/registry /root/.cargo/git target

FROM ubuntu:24.04
WORKDIR /vara-eth-node
COPY --from=builder /gear/ethexe-bin /usr/local/bin/ethexe
CMD ["ethexe", "run"]
