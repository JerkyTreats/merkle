# AGENTS.md

## Domain Architecture Rule

- Do not use `mod.rs`. Use the modern Rust convention: a module is either a single file `parent.rs` or a file `parent.rs` that declares submodules with `mod child;` and children live in `parent/child.rs`. Prefer `parent.rs` plus `parent/child.rs` over `parent/mod.rs` plus `parent/child.rs`.
- Organize code by domain first.
- Keep each domain concern under `src/<domain>/`.
- Inside a domain, name submodules by behavior, for example `query`, `mutation`, `orchestration`, `queue`, `sessions`, `sinks`.
- Keep adapters thin. `tooling` and `api` may parse, route, format, and delegate only.
- Cross domain calls must use explicit domain contracts.
- Do not reach into another domain internal modules.
- Avoid generic primary folders named by technical layer.
- For migrations, use compatibility wrappers and require characterization and parity tests before removing old paths.

## Avoid Backwards Compatibility

Maintaining backwards compatibility tends to clutter codebases, and its not a requirement for this project.

- Backwards incompatible changes are allowed when they improve domain clarity and ownership.
- Any backwards incompatible change must be called out to the user before commit.
- Commit messages must reflect change severity using conventional commit rules.
- Use `type!:` or `type(scope)!:` for breaking changes.
- Add a `BREAKING CHANGE:` footer with a concise migration impact note.
- Keep user facing impact explicit in PR notes, review notes, and release notes.

## CLI Path Default Direction

- Project intent is to make `--path` the default targeting mode for CLI flows.
- Until that rollout is complete, always call out each command surface where path is not the default behavior.
- When writing plans, specs, reviews, and implementation notes, include a short exception list for non default path behavior.

## Architecture Diagrams

- Prefer Mermaid style diagrams for architecture and workflow visuals when a diagram improves clarity.
- Keep diagrams close to the related plan or spec section so design intent and ownership boundaries remain explicit.
- Use concise labels that match domain terms used in code and design docs.


## Commits

Use `conventional commits` when instructed to commit.

- Approved commit `type` values include `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`, `ci`, `chore`, and `policy`.
- Use `policy` for repository governance updates such as standards, process rules, and enforcement workflow changes.
- For `policy` commits, include at least one governance trace footer such as `Policy-Ref:` or `Discussion:`.
- Write the subject as a declarative summary of what changed.
- Describe concrete behavior or ownership changes, not process context.
- Do not use contextual labels like phase names in the subject.
- Keep the subject focused and specific to the diff.
- Prefer `type` and `scope` with this shape `type(scope): summary`.
- Good example `refactor(provider): split provider ownership into profile repository diagnostics commands and generation`
- Bad example `refactor(provider): implement phase2`
- Breaking change example `refactor(context)!: remove legacy frame metadata compatibility path`
- Breaking footer example `BREAKING CHANGE: frame metadata no longer accepts legacy prompt key`
- Policy example `policy(agents): require policy trace footer for governance changes`

Verify with user before push.

## Parentheses Markdown Content

- Do not use literal parentheses characters `(` or `)` in Markdown prose such as headings, paragraphs, lists, and tables.
- Parentheses are allowed only when required by Markdown formatting syntax, for example `[label](/path)`, and inside inline code or fenced code blocks.
