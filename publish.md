# Publish Duet Clipboard

The owner can ask an agent:

> Great, push this to Git and build all binaries for public download. Follow publish.md.

This procedure commits the intended changes, synchronizes `main`, waits for its
checks, dispatches native Windows/macOS/Linux builds, and publishes verified
installers on GitHub Releases. The cloud build job is the "render job" in this
process; a local Windows machine does not need a Mac toolchain.

Repository: <https://github.com/bryancolvinfilm-bot/duet-clipboard>

Public downloads: <https://github.com/bryancolvinfilm-bot/duet-clipboard/releases/latest>

## Agent procedure

1. Read `git status`, the current branch, recent commits, and `origin`. Review
   the actual changes. Preserve unrelated work. Stage only the source and
   documentation intended for this release; never include local clipboard
   profiles, `.tools`, `target`, `dist`, credentials, or logs.
2. Use `main`. Fetch `origin/main` and tags. Fast-forward when behind; resolve
   divergence normally. Do not force-push or move an existing version tag.
3. Read `[package].version` in `Cargo.toml`. If this version has already been
   tagged for a different commit, choose the next patch version unless the user
   specified another version. Update `Cargo.toml`, run Cargo once without
   `--locked` to update the package's lockfile entry, and review the lockfile
   diff. Packaging derives filenames, installer versions, and Mac bundle
   versions from `Cargo.toml`; do not edit version strings in each packager.
4. Run appropriate local checks. For Rust changes, use `cargo fmt --all --
   --check`, `cargo clippy --locked --all-targets --all-features -- -D warnings`,
   and `cargo test --locked --all-targets`. The project-local Windows toolchain
   can be invoked through `scripts/dev.ps1`. Native clipboard tests overwrite
   the clipboard: leave them to the disposable CI desktops.
5. Stage the reviewed files and create a meaningful commit. Confirm the working
   tree is clean. If there are no changes, reuse the current commit.
6. Run the publisher from the repository root:

   ```powershell
   .\scripts\publish.ps1
   ```

   With PowerShell 7 on macOS/Linux, use `pwsh ./scripts/publish.ps1`.
   It requires Git, normal Git push authentication, and an existing GitHub
   sign-in through `GH_TOKEN`, `GITHUB_TOKEN`, GitHub CLI, or Git Credential
   Manager. No token is printed, saved in the repository, or added to a remote.
   On a sandboxed Windows agent, Git and Credential Manager may need to run as
   the normal Windows user through the tool's standard approval mechanism.
7. Keep following the publisher until it finishes. It fetches and fast-forwards
   if necessary, pushes `main`, checks that the remote SHA matches, and waits
   for **Check** on that exact SHA. It then dispatches **Publish** for the same
   SHA, follows every platform job, and verifies the resulting public downloads.
   Do not treat a successful push or a queued build as completion.
8. Report the source commit, release URL, and the three public installer links.
   State any genuine build or download failure instead of reporting success.
   Release signing/notarization is not configured; do not describe the builds
   as signed public production releases.

Publishing requires a user request that includes making a release/downloads.
A request only to commit or push code should stop after those actions. Once a
public release is requested, do the checks, native builds, uploads, and public
download verification without asking for the same authorization again.

## What GitHub Actions does

The workflow is `.github/workflows/build.yml`, displayed as **Test, build, and
publish**. Pushes to `main` and pull requests run checks and build artifacts;
they do not publish a release. A manual dispatch selects `main`, supplies its
full `expected_sha`, and sets `publish_release=true` to publish.

The metadata job verifies the requested commit and reads the version. Three
native runners then run formatting, all-feature Clippy, unit/integration tests,
and native clipboard round trips. They build these installers:

| Platform | Public asset |
| --- | --- |
| Windows x64 | `DuetClipboard-<version>-windows-x64-setup.exe` |
| macOS, Apple Silicon + Intel | `DuetClipboard-<version>-macos-universal.pkg` |
| Linux x86_64 | `DuetClipboard-<version>-linux-x86_64.run` |

Mac packaging verifies both architectures and the ad-hoc code signature. Linux
packaging installs into a disposable directory and runs the installed binary.
Native Linux clipboard testing uses Xvfb/X11; this does not certify every Wayland
compositor. Windows packaging uses the native MSVC runtime configuration.

Only if every platform passes does the publish job run. It downloads artifacts
from that same workflow/SHA and requires all three installers. It creates
`BUILD-INFO.json` with the source commit, version, workflow URL, sizes, and
SHA-256 hashes, plus `CHECKSUMS.sha256` covering the installers and build info.

It checks `main` has not changed, creates immutable tag `v<version>` pointing
to the tested commit, and uploads the five assets to a **draft** release. It
downloads the uploaded assets and checks their hashes before making the release
public and marking it latest. A failure leaves no partially published release.
The release notes retain the independent-project / no-Duet-Display-affiliation
notice and explain that the Windows/package builds are unsigned and the Mac app
is ad-hoc signed rather than notarized.

The local publisher then downloads those five files anonymously and verifies
them again. Its copies remain in an ignored `.tools/public-download-*` directory.
GitHub Actions artifacts are temporary build products; GitHub Release assets are
the public downloads to share with users.

## If something fails

- Check the exact workflow URL printed by the publisher. Read the failed job's
  logs, fix the cause, commit, and push. Keep meaningful native tests enabled.
- If `main` changes during a release, synchronize and publish the new commit.
  All reported assets must correspond to the same source SHA.
- Published releases and existing tags are not overwritten. A new commit needs
  a new version if the old version was already tagged. A failed upload can
  resume an existing **draft** only when its tag still points at the same SHA.
- Running the publisher again for the exact already-published commit verifies
  its downloads and returns its links instead of replacing the release.
- If authentication is missing or lacks repository/Actions permissions, use
  the normal GitHub login/permission flow. Never ask someone to paste a token
  into a public issue, chat transcript, source file, or command-line argument.
- A timeout does not cancel the cloud jobs. Inspect Actions before dispatching
  again. The publisher resumes an active matching publish run when possible.

For a GitHub CLI fallback, first push and wait for the successful **Check** run
for the exact commit. Then dispatch:

```sh
gh workflow run build.yml --ref main -f expected_sha=FULL_COMMIT_SHA -F publish_release=true
gh run list --workflow build.yml --branch main
gh run watch RUN_ID --exit-status
gh release view vVERSION
```

Verify all five public assets with `scripts/release-assets.ps1 -Mode Verify`
and the expected version/source SHA. Do not select an unrelated successful run.

References: [manual workflow dispatch](https://cli.github.com/manual/gh_workflow_run),
[release creation](https://cli.github.com/manual/gh_release_create),
[artifact handling](https://github.com/actions/upload-artifact).
