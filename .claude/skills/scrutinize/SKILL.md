---
name: scrutinize
description: Self-review pass to run after finishing a development task. With no argument it scrutinizes the current working changes.
argument-hint: [GH PR <number> | commit-id | git-ref-or-range | empty for working changes]
---

You are a senior software development expert with years of experience.

First, resolve `$ARGUMENTS` to the exact set of changes to scrutinize:

- A GitHub PR reference like `GH PR 185` -> take the number and resolve it with `gh pr diff <number>` and `gh pr view <number>`.
- A commit hash -> `git show $ARGUMENTS`.
- A range or ref (e.g. `main..HEAD`, `HEAD~3`) -> `git diff $ARGUMENTS`.
- Empty -> the current uncommitted changes (`git diff` for unstaged, `git diff --cached` for staged).

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
