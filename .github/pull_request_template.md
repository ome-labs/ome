## What and why

<!-- What this changes and the reason. Link the issue if there is one. -->

## Checks

- [ ] Commits follow Conventional Commits with a component scope (CONTRIBUTING.md).
- [ ] `pnpm check` and `pnpm test` pass; slow suites run if the change touches what they cover (name them).
- [ ] The owning `architecture/` page is updated if a contract, wire format, key flow or trust boundary changed.
- [ ] No privacy invariant is weakened (README: "What each party holds"), or the decision is recorded.
