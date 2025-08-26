FROM --platform=linux/amd64 amazonlinux:2023 as builder
LABEL description="This is the build stage for ethexe. Here we create the binary."

ARG PROFILE=release
WORKDIR /ethexe

COPY . /ethexe

# Install dependencies
RUN yum update -y && yum install -y clang gcc git gzip make tar wget unzip openssl-devel

# Install CMake v3
RUN wget https://cmake.org/files/v3.24/cmake-3.24.0-linux-x86_64.sh
RUN chmod +x cmake-3.24.0-linux-x86_64.sh
RUN ./cmake-3.24.0-linux-x86_64.sh --skip-license --prefix=/usr/local
RUN rm cmake-3.24.0-linux-x86_64.sh
RUN cmake --version

# Install Rust and toolchains
RUN wget https://sh.rustup.rs/rustup-init.sh
RUN chmod +x rustup-init.sh
RUN ./rustup-init.sh -y
ENV PATH="/root/.cargo/bin:$PATH"
RUN rustup toolchain install nightly-2025-08-26
RUN rustup target add wasm32v1-none --toolchain nightly-2025-08-26

# Build
RUN cargo build -p ethexe-cli --profile $PROFILE

# ===== SECOND STAGE ======

FROM --platform=linux/amd64 ubuntu:22.04
MAINTAINER GEAR
LABEL description="This is the 2nd stage: a very small image where we copy the ethexe binary."
ARG PROFILE=release
COPY --from=builder /ethexe/target/$PROFILE/ethexe /usr/local/bin
RUN apt-get update && apt-get install -y openssl ca-certificates
RUN useradd -m -u 1000 -U -s /bin/sh -d /ethexe ethexe && \
    mkdir -p /ethexe/.local/share && \
    mkdir /data && \
    chown -R ethexe:ethexe /data && \
    ln -s /data /ethexe/.local/share/ethexe && \
    # Sanity checks
    ldd /usr/local/bin/ethexe && \
    /usr/local/bin/ethexe --version

USER root

EXPOSE 20333 9635
CMD ["/usr/local/bin/ethexe"]
