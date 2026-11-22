# farglass

## Project memory

Project knowledge is in `.project/`. Below, `pmm` means `python3 .project/pmm`.
Query and change the records only through pmm. Read a file in `.project/`
directly only when pmm cannot answer.

- Start: `pmm brief ID --files`, one call; words of the title work instead
  of the id. It returns the record, its decisions, phase, related records,
  the focus, and the files to read with their contents. Do not search for
  the record or read those files again.
- Then make every edit, code and docs alike, in one call before you run
  anything. README changes belong in that same call.
- Finish a feature: `pmm done ID "acceptance text"`, one call. It runs the
  tests, writes Acceptance, sets done, runs check and commits as "ID: title".
- No record id yet: `pmm status`, then `pmm search TERM` or `pmm list`.
- The next planned task: `pmm focus`. Switch: `pmm focus ID`. `done` moves
  the focus to the first id under Next by itself.
- A file that brief did not name: `pmm about PATH` before you change it.
- New knowledge: run `pmm where WORDS` first, then the create command it
  prints. pmm decides the place. Do not choose or create a file yourself.
- Frontmatter: `pmm set ID key=value`. Body text: `pmm write ID Section "text"`.
  Do not edit record files by hand.
- Other commits: `pmm check` first. Commits of a record: `pmm log ID`.

Rules
- Put the record id in each commit message that works on it: "F012: add login form".
- A feature names the files it touches: `pmm set F012 files=src/a.py,src/b/`.
- A feature is done only when its Acceptance section is written and met.
- Record a decision only when it changes future work. Fill Context, Decision
  and Consequences. Never edit an accepted decision; supersede it.
- Keep NOW.md under 40 lines. No session journal. Git holds the history.
- When the work is done, report in at most three lines.
- Single words from the user: "next" means run `pmm brief --files` and do
  that task. "done" means `pmm done ID`. "status" means `pmm board`.

## Changelog, quick start, and release

- If a user or an administrator can see the change, add a bullet to
  `CHANGELOG.md` under `## Unreleased`, in `### Added`, `### Changed`, or
  `### Fixed`. Add it in the same edit call as the code, before `pmm done`.
  If `## Unreleased` is missing, add it above the topmost version heading.
- Do not add a bullet for a refactor, a test, or internal work. If you are
  not sure, add the bullet.
- Write the bullet for a farglass user: what they can do now, or what went
  wrong before and does not now. No record id, no filename, no symbol name.
- Never write a version heading in `CHANGELOG.md`. Only the `/release` skill does.
- `## Quick start` in `README.md` must always be correct. If a change touches
  a `farglassctl` command or option, the install steps, the default port, the
  supported distributions, or a key of `packaging/farglassd.conf`, update the
  quick start in the same edit call.
- Write the changelog and the README with the `scuq-scraibe` skill loaded.
- To cut a release, use the `/release` skill. Do not do it by hand. Never push
  a tag: the user does.
