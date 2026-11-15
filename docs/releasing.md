# Releasing

For a maintainer: what a release publishes, what is set up once by hand, the steps of a release, and how to build
the same packages on the command line.

## What a release publishes

- **The crate `kahlnws`** to crates.io: the library, with its examples, tests and benchmarks as sources.
- **The Python package `kahlnws`** to PyPI: a source distribution, and one wheel per platform for Linux (glibc and
  musl, x86-64 and ARM64) and macOS (x86-64 and ARM64). Each wheel serves every CPython from 3.11 on. No wheel is
  built for Windows, where the package has not been tested.

The C boundary, the Python binding's crate and the test infrastructure are not published as crates: their manifests
say `publish = false`. Every crate carries the workspace's one version.

Pushing the tag `v<version>` starts `.github/workflows/release.yml`. It checks that the tag names the workspace's
version, runs every check of a pull request on the tagged commit, packs the crate, builds the wheels and tests those
that run on the machine that built them, and only when all of that has passed publishes to both registries. Neither
registry is handed a stored secret: each publishing job proves who it is with a short-lived token GitHub issues for
this workflow (trusted publishing).

## Set up once

1. **GitHub environments.** In the repository's settings, make two environments, `crates-io` and `pypi`. A required
   reviewer on each makes every release wait for an approval before it publishes.
2. **PyPI.** In the PyPI account, under Publishing, add a pending trusted publisher for GitHub: project `kahlnws`,
   owner `FelixKahle`, repository `kahlnws`, workflow `release.yml`, environment `pypi`. The first release makes the
   project.
3. **crates.io.** A new crate's first version is published by hand with a token; trusted publishing can be set up
   only once the crate exists:

   ```sh
   cargo login
   cargo publish --locked -p kahlnws
   ```

   Then, in the crate's settings on crates.io, add a trusted publisher for GitHub: owner `FelixKahle`, repository
   `kahlnws`, workflow `release.yml`, environment `crates-io`; and revoke the token. Since the first version is then
   already on crates.io, the workflow's crates.io job fails for that one tag while its PyPI job publishes; from the
   second release on both publish.

## A release

1. Set the version in `[workspace.package]` of the root `Cargo.toml` and let cargo write it to `Cargo.lock`
   (`cargo check`).
2. In `CHANGELOG.md`, move what is under Unreleased to a section of the version and its date.
3. Merge that through a pull request, so that the checks have passed on `main`.
4. Tag the merged commit and push the tag:

   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```

5. Approve the two publishing jobs where the environments ask for it.

A version cannot be published twice to either registry. A release that failed after one registry took it is finished
by running the failed job again, or with the commands below.

## On the command line

The crate, packed and built from its packed sources alone, without publishing:

```sh
cargo package --locked -p kahlnws --list     # the files the package holds
cargo publish --locked -p kahlnws --dry-run
```

The Python package, into `crates/kahlnws_python/dist/`:

```sh
cd crates/kahlnws_python
uvx maturin build --release --locked --out dist   # the wheel of this machine
uvx maturin sdist --out dist                      # the source distribution
```

Publishing by hand needs a token of the registry: `cargo publish --locked -p kahlnws` after `cargo login`, and
`uv publish dist/*` with `UV_PUBLISH_TOKEN` set.
