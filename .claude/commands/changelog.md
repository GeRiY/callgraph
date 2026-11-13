---
description: Update CHANGELOG.md + CHANGELOG.hu.md for the current package or a release — always via the changelog-writer agent
argument-hint: "[range or version, e.g. origin/main..HEAD | release 0.9.5]"
---

Launch the `changelog-writer` agent (Agent tool, subagent_type: changelog-writer) — do NOT write changelog text
yourself, not even a one-line fix. Pass it:

- the range / task: `$ARGUMENTS` (if empty: `origin/main..HEAD`, entries go to `[Unreleased]`;
  `release X.Y.Z`: turn `[Unreleased]` into `[X.Y.Z] — <today>` and update the link list);
- anything the diff alone does not show (e.g. which changes are user-visible, what was asked by the owner).

When it returns: check `git diff CHANGELOG.md CHANGELOG.hu.md` (both files, same bullets), then report the result.
Do not commit unless the surrounding workflow says so.
