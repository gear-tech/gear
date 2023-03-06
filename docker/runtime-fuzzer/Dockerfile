FROM ubuntu:22.04

MAINTAINER GEAR

# Install dependecies
RUN apt update -y && apt install -y clang gcc git gzip wget unzip

# Install Rust and toolchains
RUN wget https://sh.rustup.rs/rustup-init.sh
RUN chmod +x rustup-init.sh
RUN ./rustup-init.sh -y
ENV PATH="/root/.cargo/bin:$PATH"
RUN rustup default stable
RUN cargo --version
RUN rustc --version
RUN rustup update nightly && rustup target add wasm32-unknown-unknown --toolchain nightly
RUN rustup default nightly

# Clone gear repo
RUN git clone https://github.com/gear-tech/gear.git

# Install cargo-fuzz
RUN cd gear
RUN cargo install cargo-fuzz

# Run the fuzzer
USER root
RUN chmod +x /gear/scripts/gear.sh
CMD ["/gear/scripts/gear.sh", "test", "fuzz"]
