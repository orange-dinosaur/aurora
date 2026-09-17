# Releasing Aurora

Releases are built on GitHub by `.github/workflows/release.yml`. Pushing `next` builds a draft. Pushing `release` publishes.

## 1. Raise the version

Change `version` in `src-tauri/Cargo.toml`. It is the only place the version lives. Then run `cargo check` in `src-tauri/` so `Cargo.lock` follows, and commit both files.

A version that is already published cannot be built again. The workflow stops and asks for a new one.

## 2. Try a build

```sh
git checkout next
git merge main
git push
```

The workflow runs the checks, builds Linux, macOS and Windows, and uploads everything to a draft named after the version. Open it on the repository's Releases page to download and try the builds. Pushing `next` again replaces the draft.

## 3. Publish

```sh
git checkout release
git merge next
git push
```

If the code matches the draft built from `next`, the workflow publishes that draft without building again. To skip the trial, merge `main` instead and it builds before publishing.

Once published, installed copies of Aurora offer the update at their next launch.

## Release notes

Write them on the draft before publishing. Keep the `<!-- tree: … -->` line in the body: the workflow uses it to recognise a draft it can publish as it is.

## The signing key

Updates are signed with the private key held in the `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repository secrets. Keep a copy somewhere safe. If it is lost, installed copies can no longer accept updates and have to be reinstalled by hand.
