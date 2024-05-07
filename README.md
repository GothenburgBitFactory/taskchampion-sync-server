TaskChampion Sync-Server
------------------------

TaskChampion is the task database [Taskwarrior](https://github.com/GothenburgBitFactory/taskwarrior) uses to store and sync tasks.
This repository implements a sync server against which Taskwarrior and other applications embedding TaskChampion can sync.

## Status

This repository was spun off from Taskwarrior itself after the 3.0.0 release.
It is still under development and currently best described as a refernce implementation of the Taskchampion sync protocol.

## Installation and usage

### Using docker

1. Set `$TASKCHAMPION_DATA_DIR` to a place where you want to keep sync server data.
2. Build and run the image:

```bash
docker build . -t taskchampion-sync-server
docker run --rm \
    -p 8080:8080 \
    --name taskchampion-sync-server \
    -e RUST_LOG=debug \
    --mount type=bind,source=$TASKCHAMPION_DATA_DIR,target=/var/lib/taskchampion-sync-server \ 
    taskchampion-sync-server
```
