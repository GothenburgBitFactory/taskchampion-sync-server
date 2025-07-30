# Introduction

Taskchampion-sync-server is an implementation of the TaskChampion [sync
protocol][sync-protocol] server. It supports synchronizing Taskwarrior tasks
between multiple systems.

The project provides both pre-built images for common use-cases (see
[usage](./usage.md)) and Rust libraries that can be used to build more
sophisticated applications ([integration](./integration.md)).

It also serves as a reference implementation: where the
[specification][sync-protocol] is ambiguous, this implementation's
interpretation is favored in resolving the ambiguity. Other implementations of
the protocol should interoperate with this implementation.

## Sync Overview

The server identifies each user with a client ID. For example, when
syncing Taskwarrior tasks between a desktop computer and a laptop, both systems
would use the same client ID to indicate that they share the same user's task data.

Task data is encrypted, and the server does not have access to the encryption
secret. The server sees only encrypted data and cannot read or modify tasks in
any way.

To perform a sync, a replica first downloads and decrypts any changes that have
been sent to the server since its last sync. It then gathers any local changes,
encrypts them, and uploads them to the server.

[sync-protocol]: https://gothenburgbitfactory.org/taskchampion/sync.html
