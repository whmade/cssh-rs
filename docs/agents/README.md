# Agent skills

Cheat sheet for people choosing which [mattpocock/skills](https://github.com/mattpocock/skills)
skill to run on cssh-rs. Agents read the configuration in `AGENTS.md`
(`## Agent skills`) and the other files in this directory.

Main flow: `/grill-with-docs` -> (`/research`, `/prototype`) -> `/to-spec`
-> `/to-tickets` -> `/implement` (drives `/tdd`, ends in
`/mattpocock-skills:code-review`) -> `/retro`.

No text a skill writes reaches GitHub before you approve its draft; see
the draft gate in `issue-tracker.md`.

## Implementing an issue

| I want to ...                             | Skills                                                                                        |
| ----------------------------------------- | --------------------------------------------------------------------------------------------- |
| Understand the issue and settle the plan  | `gh issue view <n>` -> `/grill-with-docs`                                                     |
| Answer a question the code can't answer   | `/research` (docs, APIs; posts a comment on the issue) or `/prototype` (throwaway code)       |
| Build it in one session                   | `/implement #<n>` in the same window                                                          |
| Build it over several sessions or PRs     | `/to-spec` -> `/to-tickets` -> `/clear` -> `/implement #<ticket>` per ticket                  |
| Build one behaviour test-first            | `/tdd`                                                                                        |
| Review my diff against standards and spec | `/mattpocock-skills:code-review origin/main` + spec issue                                     |
| Fix a hard bug or a flaky test            | `/diagnosing-bugs`                                                                            |
| Improve the agent setup after a session   | `/retro`, before `/clear`                                                                     |

- Keep grill -> spec -> tickets in one context window; `/clear` between
  tickets.
- One PR per ticket via `/workflows:github-pr`; stack dependent tickets
  with `gh stack`.
- Skip `/implement-spec`: it lands every ticket on one integration branch
  instead of one PR per ticket.

## Reviewing a contributor's PR

| I want to ...                                        | Skills                                                                           |
| ---------------------------------------------------- | -------------------------------------------------------------------------------- |
| Write my own spec before reading their diff          | `gh issue view <n>` -> `/grill-with-docs` -> `/to-spec`, kept as a draft         |
| Compare their change with my spec                    | `gh pr checkout <n>` -> `/mattpocock-skills:code-review origin/main` + spec path |
| Decide per deviation: is my spec or their code wrong | `/grill-with-docs` on the Spec findings                                          |
| Ask the author what I can't decide alone             | `/to-questionnaire`                                                              |
| Leave the comments                                   | `gh pr review <n>`                                                               |

- The Spec axis reports missing requirements, scope creep, and wrong
  implementations: one finding per review comment.
- Read the diff only after `/to-spec`, or the spec copies the code.

## Planning work across several issues

| I want to ...                             | Skills                                                                                       |
| ----------------------------------------- | -------------------------------------------------------------------------------------------- |
| Plan a well-scoped feature                | `/grill-with-docs` -> `/to-spec` -> `/to-tickets` (sub-issues with native blocked-by links)  |
| Plan a foggy effort beyond one session    | `/wayfinder` (chart, then one decision per session) -> `/to-spec` -> `/to-tickets`           |
| Get input from someone else               | `/to-questionnaire`                                                                          |
| Find refactoring candidates to plan first | `/improve-codebase-architecture`                                                             |
| Triage incoming issues                    | `/triage`                                                                                    |
| Ask which flow fits my situation          | `/ask-matt`                                                                                  |

- `/to-tickets` cuts vertical slices that fit one context window each; a
  ticket that reads like "Phase 1 of 3" is too big.
- When you set a v1 milestone by hand, also pick the roadmap board in the
  issue sidebar; agents only add the issues they milestone themselves.

## Changing the configuration

Edit `AGENTS.md` (`## Agent skills`) and the files in this directory
directly. Re-running `/setup-matt-pocock-skills` would write a second
block into `CLAUDE.md` without the repo overrides.
