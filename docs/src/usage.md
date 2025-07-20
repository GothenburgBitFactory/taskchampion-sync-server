# Usage

This repository is flexible and can be used in a number of ways, to suit your
needs.

- If you only need a place to sync your tasks, using cloud storage may be
cheaper and easier than running taskchampion-sync-server. See
[task-sync(5)](http://taskwarrior.org/docs/man/task-sync.5/) for details on
confusing cloud storage.

- If you have a publicly accessible server, such as a VPS, you can use `docker
compose` to run taskchampion-sync-server as pre-built docker images. See
[Docker Compose](./usage/docker-compose.md).

- If you would like more control, such as to deploy taskchampion-sync-server
within an orchestration environment such as Kubernetes, you can deploy the
docker images directly. See [Docker Images](./usage/docker-images.md).

- For even more control, or to avoid the overhead of container images, you can
build and run the taskchampion-sync-server binary directly. See
[Binaries](./usage/binaries.md).

