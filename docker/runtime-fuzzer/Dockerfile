FROM ubuntu:22.10

MAINTAINER GEAR

ENV PATH="/root/.cargo/bin:$PATH"

WORKDIR /gear
COPY . /gear

# Install dependencies
RUN apt update -y && \
    apt install -y clang gcc git gzip wget unzip; \
    # Install Rust and toolchains
    wget https://sh.rustup.rs/rustup-init.sh && \
    chmod +x rustup-init.sh && \
    ./rustup-init.sh -y ;  \
    rustup update nightly && \
    rustup target add wasm32-unknown-unknown --toolchain nightly; \
    rustup default nightly; \
    # Install cargo-fuzz \
    cargo install cargo-fuzz; \
    # Set rights \
    chmod +x ./scripts/gear.sh

# Run the fuzzer
ENTRYPOINT ["./scripts/gear.sh"]
CMD ["test", "fuzz"]