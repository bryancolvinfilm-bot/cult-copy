# Contributing

Issues and pull requests are welcome. Include the operating system, app version,
what you copied, and the expected and observed behavior. Do not post actual
clipboard contents, pairing codes, device keys, or unredacted private files.

Use the stable Rust toolchain and check changes with:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
```

Tests in `tests/native_clipboard.rs` replace the real system clipboard and are
ignored during ordinary test runs. Run them only in a disposable desktop
session. The Windows lifecycle check also needs a desktop session; see
[validation instructions](docs/VALIDATION.md).

Platform-specific changes should be checked on the affected operating system.
Keep network transfers behind device pairing and receiving consent, keep file
transfers streamed, and preserve a newer local clipboard when a download finishes.

Contributions are provided under the project's MIT License.
