FROM paritytech/ci-linux:production as builder
LABEL description="This is the build stage for Gear. Here we create the binary."

ARG PROFILE=release
WORKDIR /gear

COPY . /gear
RUN cargo build -p gear-node --$PROFILE

# ===== SECOND STAGE ======

FROM debian:buster-slim
LABEL description="This is the 2nd stage: a very small image where we copy the Gear binary."
ARG PROFILE=release
COPY --from=builder /gear/target/$PROFILE/gear-node /usr/local/bin

RUN useradd -m -u 1000 -U -s /bin/sh -d /gear gear && \
	mkdir -p /gear/.local/share && \
	mkdir /data && \
	chown -R gear:gear /data && \
	ln -s /data /gear/.local/share/gear && \
	rm -rf /usr/bin /usr/sbin

USER gear
EXPOSE 30333 9933 9944
VOLUME ["/data"]

CMD ["/usr/local/bin/gear-node"]
