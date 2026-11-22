---
id: F079
title: Changelog, release skill, and README quick start
status: done
phase:
tags: []
decisions: []
files: [CHANGELOG.md, README.md, CLAUDE.md, .claude/skills/release/, .github/workflows/release.yml]
---

## Goal
Each change that a user can see is in CHANGELOG.md, the /release skill cuts versions from it, and the README starts with a quick start that is always correct.

## Requirements
- CHANGELOG.md with an Unreleased block that lists the state before v0.1.0, in the format of chalk
- CLAUDE.md: a changelog bullet with each visible change, the quick start updated with each change to farglassctl, the port, or farglassd.conf, releases only through the skill
- .claude/skills/release/SKILL.md: version from the block (first v0.1.0), heading rename, version in Cargo.toml and web/package.json, quick start check, checks, commit; the user pushes and tags
- release.yml: the notes of the GitHub release are the changelog section of the version; no section, no release
- README.md: Quick start after the feature list (download, install with the first user, URL, more users, port, certificate, status, update); Release section names the changelog and the skill

## Acceptance
CHANGELOG.md has an Unreleased block with 22 bullets for the state before v0.1.0. The awk cut of release.yml gives 22 lines for a v0.1.0 heading and an empty result for an unknown version, and the workflow file parses as YAML. The quick start commands and options agree with crates/farglassctl/src/cli.rs, PORT 8443, and packaging/farglassd.conf. ste-lint shows no hard finding in CHANGELOG.md and the release skill. No program changed, thus the tests did not run. The release skill itself was not run: v0.1.0 is not cut yet.
