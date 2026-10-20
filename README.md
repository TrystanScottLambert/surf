# surf

GUI for surfing WAVES data products.

Surf is a desktop app for browsing the team's `waves-rips` Cloudflare R2
bucket. No setup or command line is needed. You can:

- browse every folder and dataset version (`v1/`, `v2/`, …)
- read each version's description (`<dataset>/<vN>/<dataset>.md`)
- see a table's columns and its embedded metadata (the markdown stored under
  the parquet `maml` key)
- pick columns and filter rows (e.g. `dec > 0`), preview the result, and
- download the whole file, or just your selection as Parquet or CSV.

Filtered downloads only fetch the parts of the file they need, so taking a
few columns from a large table is fast.

## Using Surf

1. Download the build for your machine from the team (macOS: `Surf.app`,
   Linux: `surf`). On macOS, the first time you must **right-click → Open**,
   because the app isn't signed with an Apple Developer ID.
2. Click a dataset on the left, then a version, to read about it.
3. Click a table, or **Explore** on the version page:
   - **Overview** shows row and column counts, metadata, and every column with its type.
   - **Select & download**:
     - tick the columns you want
     - **+ Add filter** for each condition (column, comparison, value)
     - choose whether *all* or *any* of the filters must match
     - optionally type an SQL condition in **Advanced**, e.g. `dec > 0 AND (mag_r < 19.5 OR z IS NULL)`
     - press **Update preview** to see the first 100 matching rows and the total match count
     - choose Parquet or CSV and press **Download selection**
   - **Download whole file** saves the original parquet unchanged.

Downloads appear at the bottom with progress and a cancel button. Unfinished
files are never left behind. Parquet subsets keep the table's `maml` metadata.

Logs, for troubleshooting, are in the OS data directory (macOS:
`~/Library/Application Support/org.WAVES.surf/logs`, Linux:
`~/.local/share/surf/logs`).

## Building

Requires Rust 1.94+ (`rustup update`). Linux also needs
`libgtk-3-dev libxkbcommon-dev libwayland-dev`.

```sh
cp .env.example .env      # then fill in the read-only credentials
cargo run --release -p surf-gui
```

### Credentials

Surf reads `SURF_R2_ENDPOINT` (or `SURF_R2_ACCOUNT_ID`),
`SURF_R2_ACCESS_KEY_ID`, `SURF_R2_SECRET_ACCESS_KEY` and `SURF_R2_BUCKET`
**at build time**, from the environment or a git-ignored `.env`, and embeds
them in the binary. They are XOR-masked with a random per-build key, so they
don't appear as plain text (`strings surf | grep <secret>` finds nothing).

This is **obfuscation, not encryption**. Anyone with the binary and enough
determination can recover the keys. That is acceptable only because the
token is **read-only and scoped to this one bucket**. So:

- create the R2 API token with *Object Read only* permission for `waves-rips` only;
- never commit `.env` (it's in `.gitignore`);
- share builds with the team privately. This repository is public, so the
  release workflow only creates **draft** releases (visible to collaborators,
  not the public). Don't publish them;
- if a key leaks, roll the token in the Cloudflare dashboard and rebuild.

### Checking the connection

```sh
cargo run -p surf-storage --example probe              # list the bucket root
cargo run -p surf-storage --example probe some/folder/ # list a folder
```

### Working offline / development

```sh
cargo run -p surf-query --example demo_data -- /tmp/surf-demo
SURF_LOCAL_DIR=/tmp/surf-demo cargo run -p surf-gui
```

`SURF_LOCAL_DIR` points Surf at a local folder laid out like the bucket.

## Architecture

A Cargo workspace of layered crates. Dependencies point one way, and only
the GUI knows about iced:

| Crate | Role |
|---|---|
| `surf-core` | Pure domain model, no I/O: key and version parsing, listing classification, filter model and validation, selections |
| `surf-credentials` | Build-time embedded, obfuscated credentials |
| `surf-storage` | `Bucket`: listing, text reads and streaming downloads over any `object_store` (R2, local, in-memory) |
| `surf-query` | `Engine`: footer-only metadata, `maml` extraction, filtered previews and exports with Apache DataFusion |
| `surf-gui` | The iced app. Each screen owns its state, `Message` and view, and reports to the app through an `Action` |

```
surf-gui ──► surf-query ──► surf-storage ──► surf-core
   └───────► surf-credentials
```

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

### Releasing

1. Add repository secrets `SURF_R2_ENDPOINT`, `SURF_R2_ACCESS_KEY_ID`,
   `SURF_R2_SECRET_ACCESS_KEY` and `SURF_R2_BUCKET`.
2. Bump `version` in `Cargo.toml`, then tag: `git tag v0.1.0 && git push --tags`.
3. GitHub Actions builds a universal macOS `Surf.app` and a Linux x86_64
   binary and attaches them to a draft release.

### Signing the macOS app

Without signing secrets the app gets an ad-hoc signature, and users must
right-click → Open the first time. To sign with an Apple Developer ID and
notarize, add these repository secrets:

| Secret | Value |
|---|---|
| `MACOS_CERTIFICATE_P12` | `base64 -i DeveloperID.p12 \| pbcopy`: the *Developer ID Application* certificate and its private key, exported from Keychain Access |
| `MACOS_CERTIFICATE_PASSWORD` | the password chosen when exporting the `.p12` |
| `MACOS_SIGN_IDENTITY` | e.g. `Developer ID Application: Jane Doe (ABCDE12345)` (`security find-identity -v -p codesigning`) |
| `MACOS_NOTARY_APPLE_ID` | your Apple ID email |
| `MACOS_NOTARY_TEAM_ID` | the 10-character team ID |
| `MACOS_NOTARY_PASSWORD` | an app-specific password from account.apple.com |

To sign a local build, export the `SURF_SIGN_IDENTITY` and `SURF_NOTARY_*`
variables described in `packaging/macos/bundle.sh` before running it.
