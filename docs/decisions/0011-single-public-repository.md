# ADR 0011 — Single Public Repository

- **Status:** Accepted (2026-07-04)
- **Deciders:** Project owner

## Context

Until now the project lived in two repositories: a private development repository (full
history, Turkish docs, development-process files) and a public mirror
(`oguzkabaca/SkyComet`, English docs, single-commit history). Code was synchronized from the
private side to the public side at every milestone; documentation diverged by language and
had to be bridged by hand.

In practice the two-repository model caused more problems than it solved: two working copies
on disk drifted apart, stale files accumulated in the mirror, and every milestone required a
manual, error-prone sync ritual.

## Decision

1. **One working folder, one repository.** Development happens directly in this repository
   (`oguzkabaca/SkyComet`, public). The separate mirror folder is retired.
2. **English documentation is the single documentation set.** The English docs that
   previously lived only in the public mirror are now the tracked canon.
3. **Development-process files are excluded via `.gitignore`.** Internal working notes,
   session logs, and process documents stay on the developer's disk and are never pushed.
4. **History starts from a clean initial commit.** The full pre-consolidation history is
   preserved in the (now frozen) private repository and in a local archive branch.

## Consequences

**Pros**
- No sync ritual, no drift, no duplicate working copies.
- Public visibility of the actual development history going forward.
- One documentation set to maintain.

**Cons / accepted trade-offs**
- Internal working notes are no longer version-controlled (mitigated by OS-level file backup
  and the frozen private archive).
- Pre-consolidation commit history is not visible in this repository (available in the
  private archive).

## Reversal condition

If the project ever needs private development again (e.g. embargoed security work), a
private fork can be created from `main` at that point; this ADR would then be superseded.
