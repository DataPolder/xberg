---
priority: high
---

Xberg-specific deltas over the generic `typescript-conventions` skill (from the `typescript`
builtin). Where the two disagree, this file wins.

- Lint/format is `poly lint .` / `poly fmt .`. poly embeds oxlint and oxfmt as its JS/TS
  engines — never install or invoke `oxlint`/`oxfmt`/Prettier/ESLint/Biome directly, and do
  not add a per-package lint script.
- Type checking is NOT in CI. Root `tsconfig.json` sets `strict`, `noUncheckedIndexedAccess`,
  `exactOptionalPropertyTypes`, `noPropertyAccessFromIndexSignature`, `verbatimModuleSyntax`
  and `noEmit`; it is `files: []` plus project `references` to `e2e/node` and `e2e/wasm`.
  There is no shared `extends` base — only `docs-site` extends anything.
- The root `typecheck` script (`pnpm -r --if-present run typecheck`) currently matches zero
  packages: the only two packages defining `typecheck` are under `integrations/node/`, which
  `pnpm-workspace.yaml` does not include. Treat it as a no-op until that is fixed.
- Testing is `vitest` (root devDependency, with `@vitest/coverage-v8`). No coverage threshold
  is configured anywhere — do not cite one as enforced.
- No runtime schema validator is in the dependency tree (`zod` appears in no `package.json`).
  Validate at boundaries with hand-written type guards, or propose adding a validator first.
- Two package managers coexist. The pnpm workspace (`.`, `crates/*`, `packages/**`, `e2e/**`
  minus `e2e/wasm`) uses the root `pnpm-lock.yaml` at pnpm 11.20.0. Everything under
  `integrations/node/` is outside that workspace and carries its own `package-lock.json`.
  Check which one a package belongs to before running an install.
- Bundling applies only to `integrations/node/{langchain,llamaindex}-xberg` (`tsup`). The
  binding packages are built by napi-rs (`crates/xberg-node`) and wasm-pack
  (`crates/xberg-wasm`) — do not add a bundler to them.
- `workspace:*` is used in exactly one place (`e2e/node/package.json`); prefer it for any new
  intra-workspace dependency.
- `pnpm audit` is not wired into any workflow. Dependency CVE scanning for the JS tree is
  currently manual.
