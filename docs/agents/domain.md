# Domain docs

How the engineering skills consume this repo's domain documentation.
Layout: single-context.

## Before exploring, read these

- **`GLOSSARY.md`** at the repo root.
- **`docs/adr/`**: the ADRs that touch the area you are about to work in.

When either is missing, proceed silently. `/domain-modeling` (reached via
`/grill-with-docs` and `/improve-codebase-architecture`) creates them
lazily, once a term or decision is actually resolved.

## Use the glossary's vocabulary

When your output names a domain concept (in an issue title, a refactor
proposal, a hypothesis, a test name), use the term as defined in
`GLOSSARY.md`, and keep to it over the synonyms the glossary avoids.

If the concept you need isn't in the glossary yet, that's a signal: either
you're inventing language the project doesn't use (reconsider) or there's a
real gap (note it for `/domain-modeling`).

## Flag ADR conflicts

If your output contradicts an existing ADR, surface it explicitly:

> _Contradicts ADR-0007 (<title>), but worth reopening because..._
