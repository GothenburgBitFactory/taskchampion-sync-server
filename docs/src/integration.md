# Integration

Taskchampion-sync-server can be integrated into larger applications, such as
web-based hosting services.

- Most deployments can simply use the pre-built Docker images to implement the
sync protocol, handling other aspects of the application in separate
containers. See [Pre-built Images](./integration/pre-built.md).

- More complex deployments may wish to modify or extend the operation of the
server. These can use the Rust crates to build precisely the desired
functionality. See [Rust Crates](./integration/crates.md).

- If desired, an integration may completely re-implement the [sync
protocol](https://gothenburgbitfactory.org/taskchampion/sync.html). See [Sync
Protocol Implementation](./integration/protocol-impl.md).
