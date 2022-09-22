FROM paritytech/ci-linux:staging as builder
LABEL description="This is the build stage for Gear. Here we create the binary."

ARG PROFILE=production
WORKDIR /gear

COPY . /gear
RUN apt-get update && apt-get install -y libsecp256k1-dev openssl wget
RUN rustup self uninstall -y
RUN wget https://static.rust-lang.org/rustup/archive/1.24.3/x86_64-unknown-linux-gnu/rustup-init
RUN chmod +x rustup-init
RUN ./rustup-init -y
RUN chmod +x /usr/local/cargo/bin/cargo
RUN /usr/local/cargo/bin/cargo install sccache
RUN /usr/local/cargo/bin/cargo build -p gear-node --profile $PROFILE

# ===== SECOND STAGE ======

FROM debian:stable-slim
LABEL description="This is the 2nd stage: a very small image where we copy the Gear binary."
ARG PROFILE=production
COPY --from=builder /gear/target/$PROFILE/gear-node /usr/local/bin
RUN apt-get update && apt-get install -y openssl
RUN useradd -m -u 1000 -U -s /bin/sh -d /gear gear && \
	mkdir -p /gear/.local/share && \
	mkdir /data && \
	chown -R gear:gear /data && \
	ln -s /data /gear/.local/share/gear

USER root

CMD ["/usr/local/bin/gear-node"]
