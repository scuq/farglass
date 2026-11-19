---
name: pmm
description: Work on a project memory record. /pmm ID or title words implements it. /pmm new WORDS records new knowledge. /pmm alone takes the planned focus.
argument-hint: "[ID | title words | new WORDS]"
disable-model-invocation: true
---

`pmm` means `python3 .project/pmm`. Arguments: $ARGUMENTS

If the arguments start with `new`, go to step 5. Otherwise:

1. Run `pmm brief $ARGUMENTS --files` (with no arguments it takes the focus).
   If it reports several matches, pick the one the user meant and run it again with the id.
2. Read the output once. Do not read records or the listed files again.
3. Make every edit, code and docs alike, in one call. Run nothing before all edits are done.
4. Finish. A feature: `pmm done ID "acceptance text"`. A decision: `pmm set ID status=accepted`.
   An idea: `pmm set ID status=promoted promoted_to=F###` after the feature exists.
   Report in three lines and stop.

5. New knowledge: run `pmm where WORDS` with the words after `new`.
   Run the create command it prints: a title of at most eight words, one `-s Name=text` per section,
   and `--files` for a feature. If this is the next task, run `pmm focus ID`.
   Report the id and the path in one line.
