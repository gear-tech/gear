# ===== BUILD STAGE ======
FROM amazonlinux:2023 as builder

ARG PROFILE=production
ARG TOOLCHAIN=nightly-2025-08-26

RUN yum update -y && \
    yum install -y clang gcc git gzip make tar wget unzip && \
    yum clean all && \
    rm -rf /var/cache/yum

RUN curl -OL https://github.com/protocolbuffers/protobuf/releases/download/v3.14.0/protoc-3.14.0-linux-x86_64.zip && \
    unzip -o protoc-3.14.0-linux-x86_64.zip -d /usr/local bin/protoc && \
    unzip -o protoc-3.14.0-linux-x86_64.zip -d /usr/local 'include/*' && \
    rm -f protoc-3.14.0-linux-x86_64.zip

RUN wget https://cmake.org/files/v3.24/cmake-3.24.0-linux-x86_64.sh && \
    chmod +x cmake-3.24.0-linux-x86_64.sh && \
    ./cmake-3.24.0-linux-x86_64.sh --skip-license --prefix=/usr/local && \
    rm cmake-3.24.0-linux-x86_64.sh && \
    cmake --version

RUN wget https://sh.rustup.rs/rustup-init.sh && \
    chmod +x rustup-init.sh && \
    ./rustup-init.sh -y && \
    rm rustup-init.sh

ENV PATH="/root/.cargo/bin:$PATH"

COPY . /gear

WORKDIR /gear

RUN rustup toolchain install $TOOLCHAIN && \
    rustup target add wasm32v1-none --toolchain $TOOLCHAIN

RUN cargo build -p gear-cli --profile $PROFILE


# ===== FINAL STAGE ======
FROM debian:12-slim

ARG PROFILE=production

RUN apt-get update && apt-get upgrade -y && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /gear/target/$PROFILE/gear /usr/local/bin

RUN gear --version

CMD ["gear"]
