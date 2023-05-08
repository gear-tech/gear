# This is the build stage for Gear. Here we create the binary in a temporary image.
FROM amazonlinux:2023 as builder

ARG PROFILE=production
ARG PROTOC_ZIP=protoc-3.14.0-linux-x86_64.zip

ENV PATH="/root/.cargo/bin:$PATH"

WORKDIR /gear

COPY . /gear

RUN set -eux; \
    # Install dependecies
    yum update -y ; \ 
    yum install -y clang gcc git gzip make tar wget unzip; \
    curl -OL https://github.com/protocolbuffers/protobuf/releases/download/v3.14.0/$PROTOC_ZIP && \
	unzip -o $PROTOC_ZIP -d /usr/local bin/protoc && \
	unzip -o $PROTOC_ZIP -d /usr/local 'include/*' && \
	rm -f ${PROTOC_ZIP}; \
    # Install CMake v3
    wget https://cmake.org/files/v3.24/cmake-3.24.0-linux-x86_64.sh && \
    chmod +x cmake-3.24.0-linux-x86_64.sh && \
    ./cmake-3.24.0-linux-x86_64.sh --skip-license --prefix=/usr/local && \
    rm cmake-3.24.0-linux-x86_64.sh && \
    cmake --version; \
    # Install Rust and toolchains
    wget https://sh.rustup.rs/rustup-init.sh && \
     chmod +x rustup-init.sh && \
     ./rustup-init.sh -y ; \
	#RUN rustup default stable
    rustup toolchain install nightly-2023-04-25 && \
    rustup target add wasm32-unknown-unknown --toolchain nightly-2023-04-25
#RUN rustup update nightly && rustup target add wasm32-unknown-unknown --toolchain

RUN cargo build -p gear-cli --profile $PROFILE

# This is the 2nd stage: a very small image where we copy the Gears binary."
FROM ubuntu:22.10

ARG PROFILE=production

LABEL description="Multistage Docker image for Gear: a platform for Smart Contracts" \
	  image.type="builder" \
	  image.authors="gear@gear.foundation" \
	  image.vendor="Gear Technologies" \
	  image.description="Gear: a platform for Smart Contracts" \
	  image.source="https://github.com/gear-tech/gear/blob/master/docker/Dockerfile" \
	  image.documentation="https://wiki.gear-tech.io/docs/"

COPY --from=builder /gear/target/$PROFILE/gear /usr/local/bin

RUN apt-get update; \
    apt-get install -y openssl; \
    useradd -m -u 1000 -U -s /bin/sh -d /gear gear && \
	mkdir -p /data /gear/.local/share && \
	chown -R gear:gear /data && \
	ln -s /data /gear/.local/share/gear && \
# unclutter and minimize the attack surface
#	rm -rf /usr/bin /usr/sbin && \
# check if executable works in this container
	/usr/local/bin/gear --version

USER gear

EXPOSE 30333 9933 9944
VOLUME ["/data"]

CMD ["/usr/local/bin/gear"]