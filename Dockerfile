# Versions must be major.minor
# Default versions are as below
ARG RUST_VERSION=1.78
ARG ALPINE_VERSION=3.19

FROM docker.io/rust:${RUST_VERSION}-alpine${ALPINE_VERSION} AS builder
COPY Cargo.lock Cargo.toml /data/
COPY core /data/core/
COPY server /data/server/
COPY sqlite /data/sqlite/
RUN apk -U add libc-dev && \
  cd /data && \
  cargo build --release

FROM docker.io/alpine:${ALPINE_VERSION}
COPY --from=builder /data/target/release/taskchampion-sync-server /bin
RUN apk add --no-cache su-exec && \
  adduser -u 1092 -S -D -H -h /var/lib/taskchampion-sync-server -s /sbin/nologin -G users \
  -g taskchampion taskchampion && \
  install -d -m1755 -o1092 -g1092 "/var/lib/taskchampion-sync-server"
EXPOSE 8080
VOLUME /var/lib/taskchampion-sync-server/data
COPY docker-entrypoint.sh /bin
ENTRYPOINT [ "/bin/docker-entrypoint.sh" ]
CMD [ "/bin/taskchampion-sync-server" ]
