# Release process

1. Run `git pull upstream main`
1. Run `cargo test`
1. Run `cargo clean && cargo clippy`
1. Remove the `-pre` from `version` in all `*/Cargo.toml`, and from the `version = ..` in any references between packages.
1. Update the link to `docker-compose.yml` in `README.md` to refer to the new version.
1. Update the docker image in `docker-compose.yml` to refer to the new version.
1. Run `cargo semver-checks` (https://crates.io/crates/cargo-semver-checks)
1. Run `cargo build --release`
1. Commit the changes (Cargo.lock will change too) with comment `vX.Y.Z`.
1. Run `git tag vX.Y.Z`
1. Run `git push upstream`
1. Run `git push upstream --tag vX.Y.Z`
1. Run `cargo publish -p taskchampion-sync-server-core`
1. Run `cargo publish -p taskchampion-sync-server-storage-sqlite` (and add any other new published packages here)
1. Bump the patch version in `*/Cargo.toml` and add the `-pre` suffix. This allows `cargo-semver-checks` to check for changes not accounted for in the version delta.
1. Run `cargo build --release` again to update `Cargo.lock`
1. Commit that change with comment "Bump to -pre version".
1. Run `git push upstream`
1. Navigate to the tag in the GitHub releases UI and create a release with general comments about the changes in the release

---

For the next release,

- remove postgres from the exclusion list in `.github/workflows/checks.yml` after the release

- include the folowing in the release notes:

Running the Docker image for this server without specifying DATA_DIR
defaulted to storing the server data in
`/var/lib/taskchampion-sync-server`. However, the Dockerfile only
specifies that the subdirectory `/var/lib/taskchampion-sync-server/data`
is a VOLUME. This change fixes the default to match the VOLUME, putting
the server data on an ephemeral volume or, if a `--volume
$NAME:/var/lib/taskchampion-sync-server/data` argument is provided to
`docker run`, in a named volume.

Before this commit, with default settings the server data is stored in
the container's ephemeral writeable layer. When the container is killed,
the data is lost. This issue does not affect deployments with `docker
compose`, as the compose configuration specifies a correct `DATA_DIR`.

You can determine if your deployment is affected as follows. First,
determine the ID of the running server container, `$CONTAINER`. Examine
the volumes for that container:

```shell
$ docker container inspect $CONTAINER | jq '.[0].Config.Volumes'
{
  "/var/lib/task-champion-sync-server/data": {}
}
```

Next, find the server data, in a `.sqlite3` file:

```shell
$ docker exec $CONTAINER find /var/lib/taskchampion-sync-server
/var/lib/taskchampion-sync-server
/var/lib/taskchampion-sync-server/data
/var/lib/taskchampion-sync-server/taskchampion-sync-server.sqlite3
```

If the data is not in a directory mounted as a volume, then it is
ephemeral. To copy the data out of the container:

```shell
docker cp $CONTAINER:/var/lib/taskchampion-sync-server/taskchampion-sync-server.sqlite3 /tmp
```

You may then upgrade the image and use `docker cp` to copy the data back
to the correct location, `/var/lib/taskchampion-sync-server/data`.

Note that, as long as all replicas are fully synced, the TaskChampion
sync protocol is resilient to loss of server data, so even if the server
data has been lost, `task sync` may continue to work.
