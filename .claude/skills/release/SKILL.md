---
name: release
description: Cut a farglass release. Rename the CHANGELOG `## Unreleased` block to a version heading, set the version in Cargo.toml and web/package.json, check the README quick start, commit, and print the push and tag commands for the user to run. Use when asked to release, cut a version, or tag vX.Y.Z.
---

A farglass release is a changelog heading, a version number in the source, and a tag.
The tag `vX.Y.Z` starts `.github/workflows/release.yml`.
The workflow builds the tarballs and `farglassctl` for amd64 and arm64, signs `SHA256SUMS` with cosign, and publishes the GitHub release.
The notes of the GitHub release are the `## vX.Y.Z` section of `CHANGELOG.md`.
If the section is missing, the workflow publishes nothing.

The version is in two places, and the two must agree with the tag:

- `farglassctl` takes its version from the tag (`FARGLASS_VERSION` in `packaging/release.sh`).
- `farglassd`, `farglass-login`, and `farglass-session` take it from `version` in `[workspace.package]` of `Cargo.toml`.

You make the release commit.
Never run `git push`, `git tag`, or `gh workflow run`. Print the commands, and the user runs them.
The tag publishes a signed release that each host can install, and you cannot take it back.

## Steps

1. Make sure that there is something to release.
   Read the `## Unreleased` block in `CHANGELOG.md`.
   If it is empty or missing, stop and say so.
   Make sure that `git status` shows a clean tree on `main`.

2. Select the version, unless the user named one.
   Run `git tag --list 'v*' | tail -5`.
   - No tag: the version is v0.1.0.
   - Only `### Fixed` in the block: increase the patch number (0.1.0 to 0.1.1).
   - An `### Added` or a `### Changed` in the block: increase the minor number (0.1.1 to 0.2.0).

   The topmost `## vX.Y.Z` heading in `CHANGELOG.md` is the last release.

3. Rename the heading.
   `## Unreleased` becomes `## vX.Y.Z — <D Month YYYY> — <theme>`, for example
   `## v0.2.0 — 4 December 2026 — Lock screen and file manager`.
   Write the day with no leading zero.
   The theme is a short phrase in sentence case that names the subject of the release, from the bullets.
   Do not use a record id or a filename in the theme.
   Do not add a new empty `## Unreleased` block. The next change adds it.
   The version must be the second word of the heading: the release workflow finds the section by it.

   If the topmost heading has the date of today and the same subject, ask the user if the new bullets go into that heading.

4. Set the version in the source, if it is not X.Y.Z already:
   - Set `version = "X.Y.Z"` in `[workspace.package]` of `Cargo.toml`.
   - Run `cargo check --workspace` to write `Cargo.lock`.
   - Run `npm --prefix web version X.Y.Z --no-git-tag-version` to write `web/package.json` and `web/package-lock.json`.

5. Check the README quick start against the source.
   Compare each command, option, path, and port in `## Quick start` of `README.md` with:
   - `crates/farglassctl/src/cli.rs` (commands and options)
   - `PORT` in `crates/farglassctl/src/install.rs`
   - The supported distributions in `crates/farglassctl/src/os.rs`
   - `packaging/farglassd.conf` (keys and defaults)

   Correct each difference in the README in this change set.

   farglass supports the newest release and the release before it of Debian, Ubuntu LTS, RHEL, and Fedora.
   If a distribution has a newer release than the README names, tell the user before you continue.
   A change of the list touches `SUPPORTED` and `supported_text` in `os.rs`, the smoke test images in `release.yml`, the README, and the changelog.
   If the tag example in `## Release` of the README names an older version than X.Y.Z, leave it. It is an example.

6. Run these checks. Do not continue if a command fails.

   ```sh
   cargo test --workspace
   cargo clippy --workspace --all-targets
   npm --prefix web test
   npm --prefix web run build
   cargo audit
   npm --prefix web audit --omit=dev
   python3 .project/pmm check
   ```

   If `cargo` reports "No space left on device", run `cargo clean` and start the checks again.

7. Commit. Add only the files of the release:

   ```sh
   git add CHANGELOG.md README.md Cargo.toml Cargo.lock web/package.json web/package-lock.json
   git commit -m "release vX.Y.Z: <theme, lowercase>"
   ```

   A release commit has no record id. This is the one exception to the commit rule in `CLAUDE.md`.

8. Print these commands in one block for the user. Do not run them.

   ```sh
   git push
   git tag -a vX.Y.Z -m "vX.Y.Z — <theme>"
   git push origin vX.Y.Z
   ```

   The tag is annotated (`-a`), never lightweight.

9. Tell the user what comes next.
   The tag push starts the release workflow.
   The workflow builds, does the smoke tests on Debian 12, Ubuntu 24.04, Rocky Linux 9, and Fedora 44, signs, and publishes.
   A host gets the release with `sudo farglassctl update`.

10. If the user reports that no run started in one minute, GitHub dropped the tag event.
    Check with `gh run list --workflow=release --limit 3`.
    Then give the user this command, at the tag:

    ```sh
    gh workflow run release.yml --ref vX.Y.Z
    ```

    Never `--ref main`.
    The cosign certificate records the ref of the run, and `farglassctl` accepts only `release.yml@refs/tags/v*` (`crates/farglassctl/src/release.rs`).
    The `meta` job of the workflow stops a run from a branch before it builds.

## If the release workflow fails

- Before the `release` job published: correct the fault on `main` with a usual commit.
  Then the user moves the tag: `git tag -d vX.Y.Z`, `git push origin :refs/tags/vX.Y.Z`, and the tag commands of step 8 again.
- After the GitHub release exists: do not move the tag. Hosts can have that release. Cut the next patch version.
