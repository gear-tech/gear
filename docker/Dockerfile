FROM amazonlinux:2023 as builder
LABEL description="This is the build stage for Gear. Here we create the binary."

ARG PROFILE=production
WORKDIR /gear

COPY . /gear

# Install dependecies
RUN yum update -y && yum install -y clang gcc git gzip make tar wget unzip

ARG PROTOC_ZIP=protoc-3.14.0-linux-x86_64.zip
RUN curl -OL https://github.com/protocolbuffers/protobuf/releases/download/v3.14.0/$PROTOC_ZIP
RUN unzip -o $PROTOC_ZIP -d /usr/local bin/protoc
RUN unzip -o $PROTOC_ZIP -d /usr/local 'include/*'
RUN rm -f $PROTOC_ZIP

# Install CMake v3
RUN wget https://cmake.org/files/v3.24/cmake-3.24.0-linux-x86_64.sh
RUN chmod +x cmake-3.24.0-linux-x86_64.sh
RUN ./cmake-3.24.0-linux-x86_64.sh --skip-license --prefix=/usr/local
RUN rm cmake-3.24.0-linux-x86_64.sh
RUN cmake --version

# Install Rust and toolchains
RUN  wget https://sh.rustup.rs/rustup-init.sh
RUN chmod +x rustup-init.sh
RUN ./rustup-init.sh -y
ENV PATH="/root/.cargo/bin:$PATH"
RUN rustup default stable
RUN cargo --version
RUN rustc --version
RUN rustup update nightly && rustup target add wasm32-unknown-unknown --toolchain nightly

# Build
RUN cargo build -p gear-cli --profile $PROFILE

# ===== SECOND STAGE ======

FROM ubuntu:22.10
MAINTAINER GEAR
LABEL description="This is the 2nd stage: a very small image where we copy the Gear binary."
ARG PROFILE=production
COPY --from=builder /gear/target/$PROFILE/gear /usr/local/bin
RUN apt-get update && apt-get install -y openssl
RUN useradd -m -u 1000 -U -s /bin/sh -d /gear gear && \
	mkdir -p /gear/.local/share && \
	mkdir /data && \
	chown -R gear:gear /data && \
	ln -s /data /gear/.local/share/gear

USER root

CMD ["/usr/local/bin/gear"]
