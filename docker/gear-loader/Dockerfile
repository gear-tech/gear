FROM docker.io/paritytech/ci-linux:production as builder

WORKDIR /loader
COPY . .
RUN cargo build --locked --package gear-node-loader

FROM docker.io/library/ubuntu:20.04
LABEL description="Multistage Docker image for Gear Node Loader" \
  image.type="builder" \
  image.authors="netovannuyrikjo@gmail.com" \
  image.description="Multistage Docker image for Gaer Node Loader" \
  image.source="https://github.com/gear-tech/gear" 

COPY --from=builder /loader/target/debug/gear-node-loader /usr/local/bin

RUN useradd -m -u 1000 -U -s /bin/sh -d /node-loader node-loader
USER node-loader

ENTRYPOINT ["/usr/local/bin/gear-node-loader"]