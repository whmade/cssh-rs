---
name: scrutinize
description: Self-review pass to run after finishing a development task. Critically challenge every character added by a PR, commit, or diff - remove what is not needed, simplify what is, and tighten comments and docs. With no argument it scrutinizes the current working changes.
argument-hint: [pr-number | commit-id | git-ref-or-range | empty for working changes]
---

You are a senior software development expert with years of experience.

First, resolve `$ARGUMENTS` to the exact set of changes to scrutinize:

- A bare number (e.g. `185`) -> a GitHub PR: `gh pr diff $ARGUMENTS` and `gh pr view $ARGUMENTS`.
- A commit hash -> `git show $ARGUMENTS`.
- A range or ref (e.g. `main..HEAD`, `HEAD~3`) -> `git diff $ARGUMENTS`.
- Empty -> the current uncommitted changes (`git diff` for unstaged, `git diff --cached` for staged).

Read the resolved diff in full before changing anything. Read the relevant
surrounding code so a "simpler" rewrite stays correct, and honor the project's
own conventions in `AGENTS.md` / `CLAUDE.md`.

Then critically challenge each and every single character added by the change:

- If it is not absolutely needed, remove it.
- If it can be done more simply, make it simpler.
- If it can be done more elegantly, make it more elegant.

While doing so, keep the result readable:

- No abbreviations.
- Variables have speaking, descriptive names.
- The additions stay readable and well documented.

But hold comments and documentation to the same standard - critically
challenge each comment and each doc string. If it can be shorter, more
precise, and less prose, make it so.

Apply the changes directly. For every edit, state what you challenged and why
the change is justified (removed as unneeded / simpler / more elegant /
tightened prose). If a change is genuinely needed as-is, say so rather than
inventing a change.

After editing, run the project's checks for any modified code and report the
results.
