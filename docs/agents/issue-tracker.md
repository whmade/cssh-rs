# Issue tracker: GitHub

Issues and specs for this repo live as GitHub issues on `whmade/cssh-rs`.
Use the `gh` CLI for all operations.

## Draft gate

The repo is public, so every text a skill writes to GitHub (issue, comment,
edit of a body or title) passes through a draft first. Writes without text
(labels, assignees, closing, the board) skip it.

1. Write the exact text to publish to `.scratch/<slug>/` in the current
   worktree: `spec.md` for a spec, `issues/<NN>-<slug>.md` per ticket
   (numbered blockers first), `comments/<NN>-<slug>.md` per comment,
   `edits/<NN>-<slug>.md` per body edit (the issue's full new body).
   Everything else goes in `.scratch/<slug>/index.md`, one entry per
   draft, so it is approved with the text: an issue's title, labels,
   milestone, parent, and blockers; the issue a comment or edit targets;
   a new title.
2. Show the index and the drafts and revise them for as many rounds as the
   user asks; only an explicit approval to publish ends the loop.
3. Publish each file as is (`--body-file <draft>`), then record the
   resulting issue number or comment URL in its `index.md` entry.

A subagent only writes its draft; the session talking to the user runs
steps 2 and 3.

When a skill says "publish to the issue tracker", run the draft gate, then
**Create** below. When a skill says "fetch the relevant ticket", **Read** it.

## Conventions

- **Title** of a new issue (older titles are not a model): roadmap work
  is `[M<n>] <summary>`; everything else is `<scope>: <summary>` with a
  lowercase scope from existing issues (`client:`, `daemon:`, `xtask:`).
- **Milestone**: roadmap work carries its `M<n>: ...` milestone; a
  sub-issue takes its parent's milestone; any other issue gets one only
  when the user assigns it. An issue with a v1 milestone (`M0` to `M10`)
  belongs on the "cssh-rs v1 Roadmap" board; see **Add to board**.
- **Labels**: the ones from `.github/labels.yml` that fit; a new label
  needs a PR to that file. Wayfinder maps and tickets carry only their
  `wayfinder:*` label.

## Operations

The sub-issue and blocking flags need `gh` 2.94 or newer.

- **Create**: `gh issue create --title "..." --body-file <draft>`, plus
  `--label`, `--milestone "<full milestone title>"`, `--parent <n>`, and
  `--blocked-by <n>,<n>` as drafted.
- **Add to board**: right after an issue gets a v1 milestone (at creation
  or later), `gh project item-add 1 --owner whmade --url <issue-url>`. The
  call needs a token with Projects write; when it fails on permissions,
  give the user the issue URL to add by hand.
- **Read**: `gh issue view <n> --json number,title,body,labels,milestone,comments,parent,subIssues,blockedBy`.
- **List**: `gh issue list --state open --json number,title,body,labels,milestone,comments`
  with `--label`, `--milestone`, and `--state` filters.
- **Comment**: `gh issue comment <n> --body-file <draft>`.
- **Edit**: `gh issue edit <n> --body-file <draft>` for a new body,
  `--title "..."` for a new title.
- **Apply / remove labels**: `gh issue edit <n> --add-label "..."` /
  `--remove-label "..."`.
- **Close**: comment first if there is something to say, then
  `gh issue close <n>`.
- **Sub-issue**: `gh issue edit <child> --parent <parent>`.
- **Blocking**: GitHub's native issue dependencies,
  `gh issue edit <blocked> --add-blocked-by <n>,<n>`. An issue is
  unblocked once every issue in its `blockedBy` is closed.

## Research findings

`/research` and wayfinder `research` tickets publish one comment on the
related issue, through the draft gate. That comment is the only artifact
of the research. It is:

- **Concise**: the answer and the facts the decision needs, in a few short
  paragraphs or bullets.
- **High-level**: conclusions, not the reading trail.
- **Self-contained**: readable without the session or any local file;
  each claim links its primary source.

When no related issue exists yet, report the findings in the session and
fold them into the spec or ticket draft.

## Pull requests as a triage surface

**PRs as a request surface: no.** _(Set to `yes` if this repo treats
external PRs as feature requests; `/triage` reads this flag.)_

When set to `yes`, PRs run through the same labels and states as issues,
using the `gh pr` equivalents: `gh pr view <n> --comments`, `gh pr diff <n>`,
`gh pr comment`, `gh pr edit --add-label` / `--remove-label`, `gh pr close`.
Triage lists only external PRs, from anyone but an owner, member, or
collaborator:
`gh api --paginate 'repos/{owner}/{repo}/pulls?state=open' --jq '.[] | select(.author_association | IN("OWNER","MEMBER","COLLABORATOR") | not) | {number, title, author: .user.login, author_association, labels: [.labels[].name]}'`.
GitHub shares one number space across issues and PRs, so resolve a bare
`#42` with `gh pr view 42` and fall back to `gh issue view 42`.

## Wayfinding operations

Used by `/wayfinder`. The **map** is a single issue with **child** issues
as tickets. Charting publishes the map and its tickets as one batch
through the draft gate.

- **Map**: an issue labelled `wayfinder:map`.
- **Child ticket**: a sub-issue of the map, labelled `wayfinder:<type>`
  (`research`, `prototype`, `grilling`, or `task`), and assigned to the
  driving dev once claimed.
- **Blocking**: native issue dependencies, as in **Blocking** above.
- **Frontier query**: the map's open sub-issues
  (`gh issue view <map> --json subIssues`), minus any with an `OPEN` entry
  in `blockedBy` or an assignee (`gh issue view <n> --json blockedBy,assignees`);
  first in map order wins.
- **Claim**: `gh issue edit <n> --add-assignee @me`, the session's first
  write.
- **Resolve**: the resolution comment (a `research` ticket's is its
  research-findings comment), `gh issue close <n>`, then a context pointer
  (gist + link) appended to the map's Decisions-so-far; the comment and the
  map edit go through the draft gate together.
