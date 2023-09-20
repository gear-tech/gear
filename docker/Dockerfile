FROM debian:12-slim

ARG RELEASE_VERSION

RUN apt-get update && \
    apt-get install -y curl sudo xz-utils && \
    rm -rf /var/lib/apt/lists/*

RUN if [ -z "$RELEASE_VERSION" ]; then \
        curl --proto '=https' --tlsv1.2 -sSf https://get.gear.rs/install.sh | bash -s -- --to /usr/local/bin/ ; \
    else \
        curl --proto '=https' --tlsv1.2 -sSf https://get.gear.rs/install.sh | bash -s -- --tag ${RELEASE_VERSION} --to /usr/local/bin/ ; \
    fi

RUN gear --version

CMD ["gear"]
