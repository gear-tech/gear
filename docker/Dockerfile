FROM ubuntu:latest
LABEL description="Gear container"

WORKDIR /gear

# Install dependecies
RUN apt-get update -y && apt-get install -y clang gcc git gzip make tar wget unzip curl apt-utils

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
RUN wget https://sh.rustup.rs/rustup-init.sh
RUN chmod +x rustup-init.sh
RUN ./rustup-init.sh -y
ENV PATH="/root/.cargo/bin:$PATH"
RUN rustup default stable
RUN cargo --version
RUN rustc --version
RUN rustup update nightly && rustup target add wasm32-unknown-unknown --toolchain nightly

RUN sh -c "$(wget -O- https://github.com/deluan/zsh-in-docker/releases/download/v1.1.3/zsh-in-docker.sh)"
