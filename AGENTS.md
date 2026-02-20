# AGENTS.md

## Domain Architecture Rule

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


## Commits

Use `conventional commits` when instructed to commit.

- Write the subject as a declarative summary of what changed.
- Describe concrete behavior or ownership changes, not process context.
- Do not use contextual labels like phase names in the subject.
- Keep the subject focused and specific to the diff.
- Prefer `type` and `scope` with this shape `type(scope): summary`.
- Good example `refactor(provider): split provider ownership into profile repository diagnostics commands and generation`
- Bad example `refactor(provider): implement phase2`

Verify with user before push.

## Parentheses Markdown Content

- Do not use literal parentheses characters `(` or `)` in Markdown prose such as headings, paragraphs, lists, and tables.
- Parentheses are allowed only when required by Markdown formatting syntax, for example `[label](/path)`, and inside inline code or fenced code blocks.