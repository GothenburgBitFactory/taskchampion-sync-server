# Versions must be major.minor
ARG RUST_VERSION=1.77
ARG ALPINE_VERSION=3.19

FROM docker.io/rust:${RUST_VERSION}-alpine${ALPINE_VERSION} AS builder
COPY . /data
RUN apk -U add libc-dev && \
  cd /data && \
  cargo build --release

FROM docker.io/alpine:${ALPINE_VERSION}
COPY --from=builder /data/target/release/taskchampion-sync-server /bin
RUN adduser -S -D -H -h /var/lib/taskchampion-sync-server -s /sbin/nologin -G users \
  -g taskchampion taskchampion && \
  install -d -m755 -o100 -g100 "/var/lib/taskchampion-sync-server"
EXPOSE 8080
VOLUME "/var/lib/taskchampion-sync-server"
USER taskchampion
ENTRYPOINT [ "taskchampion-sync-server" ]
