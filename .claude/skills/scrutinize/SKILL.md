---
name: scrutinize
description: Self-review pass to run after finishing a development task. With no argument it scrutinizes the current working changes.
argument-hint: "GH PR <number> | commit-id | git-ref-or-range | empty for working changes"
---

You are a senior software development expert with years of experience.

First, resolve `$ARGUMENTS` to the exact set of changes to scrutinize. Pick the
single case that matches and ignore the rest:

- A GitHub PR reference like `GH PR 185` -> take the number `N` and resolve it
  with `gh pr diff N` for the diff and
  `gh pr view N --json number,title,body,state,headRefName,baseRefName,url,author,files,commits`
  for the metadata. Do NOT use a bare `gh pr view N`: its default view fetches
  classic project cards and fails with a "Projects (classic) is being
  deprecated" GraphQL error.
- A commit hash -> `git show <hash>`.
- A range or ref (e.g. `main..HEAD`, `HEAD~3`) -> `git diff <range-or-ref>`.
- Empty -> the current uncommitted changes (`git diff` for unstaged, `git diff
  --cached` for staged).

When the target is a GitHub PR, you must edit the PR's actual code, not the tip
of whatever branch happens to be checked out. Get the PR code into the CURRENT
worktree before reading or editing:

- NEVER `cd` into another worktree, even if `git worktree list` shows the PR
  branch checked out elsewhere. Those are separate paseo workspaces - stay in
  the current one and do all work here.
- Record the current branch first (`git rev-parse --abbrev-ref HEAD`) so you can
  return to it afterward.
- Check out the PR head as a DETACHED HEAD in the current worktree:
  `gh pr checkout N --detach`. Detaching is what lets this work even when the
  PR's branch is already checked out in another worktree - it avoids the
  "branch is already used by worktree" conflict. If `gh` is unavailable, use
  `git fetch origin pull/N/head` followed by `git checkout --detach FETCH_HEAD`.
- Spot-check that one changed file already contains the PR's changes before you
  edit it. If a file still shows the old code, you are not on the PR code - fix
  that before editing.

Read the resolved diff in full before changing anything, together with all
supporting material: any linked GitHub issue, existing PR or commit comments,
and related discussion. Then read the surrounding code so a "simpler" rewrite
stays correct and so you understand the project's conventions and structure,
and honor `AGENTS.md` / `CLAUDE.md`.

Then research online for current best practices and state-of-the-art
approaches for exactly what the change is trying to do, so every decision that
follows is well founded.

Now critically challenge each and every single character added by the change:

- If it is not absolutely needed, remove it.
- If it can be done more simply, make it simpler.
- If it can be done more elegantly, make it more elegant.

While doing so, keep the result readable:

- No abbreviations.
- Variables have speaking, descriptive names.
- The additions stay readable.

Hold comments and docstrings to a strict standard:

- Good code needs no inline comments. Add one only to explain something
  non-obvious that the code cannot convey on its own.
- A good inline comment is at most one line - a line, not a sentence.
- The same limits apply to docstrings: explain only what is not obvious, as
  briefly as possible.
- Honor the project's documentation standards while trimming. Where
  `AGENTS.md` mandates structure (e.g. cssh keeps the `# Arguments` and
  `# Returns` sections on public docstrings), keep that structure but make each
  entry as short and precise as possible.

Challenge every existing comment and docstring against these limits and cut or
shorten anything that fails them.

Apply the changes directly. For every edit, state what you challenged and why
the change is justified (removed as unneeded / simpler / more elegant /
tightened prose). If a change is genuinely needed as-is, say so rather than
inventing a change.

Once the edits are applied, verify them with the project's own checks (for
cssh: `cargo fmt`, `cargo lint`, `cargo xtask check-typography`, and the test
commands) before recording anything.

Finally, publish the result so the scrutiny is not stranded locally:

- For a GitHub PR: commit the edits following the project's commit conventions
  (for cssh, the `commit` skill, including the mandatory `Co-authored-by`
  trailer for AI edits). Because you are on a detached HEAD, push the new commit
  to the PR's branch explicitly: `git push origin HEAD:<headRefName>`, using the
  `headRefName` from the `gh pr view --json` metadata. This updates the PR
  automatically. Do this without being asked - updating the PR is part of the
  task. Then return to the branch you recorded earlier
  (`git checkout <original-branch>`), report the new commit hash, and confirm
  the PR now reflects the scrutiny.
- For a commit on a branch: commit the edits per the project's conventions and
  `git push` so the branch is updated.
- For working changes (empty argument): leave them staged/unstaged as you found
  them and do not commit unless asked.
