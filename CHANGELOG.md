# Changelog

## [1.1.0](https://github.com/JerkyTreats/meld/compare/v1.0.2...v1.1.0) (2026-03-02)


### Features

* **logging:** enable default file logging with cross platform path resolution ([f3cc1ee](https://github.com/JerkyTreats/meld/commit/f3cc1ee012692d6ca23d5740e714e29af9b641e2))


### Bug Fixes

* **context:** ground file generation prompts on source content ([06bd0a2](https://github.com/JerkyTreats/meld/commit/06bd0a21c93ed57b0b8bb6185c1551d468610e16))

## [1.0.2](https://github.com/JerkyTreats/meld/compare/v1.0.1...v1.0.2) (2026-02-27)


### Bug Fixes

* **provider:** add default request wait timeouts ([458a98f](https://github.com/JerkyTreats/meld/commit/458a98f8ec3fc2ef87aeb810ae5e791fab528718))
* **provider:** infer https for local endpoints and prompt local api key ([f3f40c9](https://github.com/JerkyTreats/meld/commit/f3f40c97b4cf87ecfe6de7acaea2e9fabaa9be0a))

## [1.0.1](https://github.com/JerkyTreats/meld/compare/v1.0.0...v1.0.1) (2026-02-26)


### Bug Fixes

* **ci:** attempt to align crates/release-please version ([2b6eaad](https://github.com/JerkyTreats/meld/commit/2b6eaadda9c6ce97efe551e87b5e2dcf4a24f7ac))
* **prompt:** add better prompt of docs-writer ([ba50c85](https://github.com/JerkyTreats/meld/commit/ba50c85b4a5204293018e9039edf2f7b197fbc8d))

## [0.1.1](https://github.com/JerkyTreats/meld/compare/v0.1.0...v0.1.1) (2026-02-25)


### Bug Fixes

* **ci:** correct(?) release repo ([24d05ad](https://github.com/JerkyTreats/meld/commit/24d05ad9ccd18430fcfd827f01e05f54f394ca84))

## 0.1.0 (2026-02-25)


### ⚠ BREAKING CHANGES

* Version 1.0

### Features

* Add baseline tests for refactor ([2df72f8](https://github.com/JerkyTreats/meld/commit/2df72f8377b03bfb5261d55abf6092f64b8c0a5c))
* **agent:** add prompt show and prompt edit commands ([e5dd894](https://github.com/JerkyTreats/meld/commit/e5dd894b9b19fa219b766c666d74560aa4c605a0))
* **context:** Add Context Orchestration ([684b62a](https://github.com/JerkyTreats/meld/commit/684b62a4020c2818925aa83a47293c7020858a33))
* **context:** Add regenerate alias for generate --force --no-recursive ([41f684f](https://github.com/JerkyTreats/meld/commit/41f684fff65a77d4a8f0c38f196d23a7dbf81a25))
* **context:** include child context in directory generation ([26faa5f](https://github.com/JerkyTreats/meld/commit/26faa5fac9c7f09a0871516e4461025201d65e19))
* Remove regeneration feature ([f9b098f](https://github.com/JerkyTreats/meld/commit/f9b098f1a34e402f0a6c2dd884fc1e0eb3b6ab5f))
* Remove synthesis feature ([fe031a9](https://github.com/JerkyTreats/meld/commit/fe031a91403d370268b30b888497aab4d27818c8))
* Version 1.0 ([6e13fe3](https://github.com/JerkyTreats/meld/commit/6e13fe3843c52c8033e2785feab36fac1e717c83))


### Bug Fixes

* Add missing summary event families ([f1d2c72](https://github.com/JerkyTreats/meld/commit/f1d2c7220e3408e133a41c8afa291194d959784b))
* **agent:** show resolved prompt path in agent show output ([2a02681](https://github.com/JerkyTreats/meld/commit/2a02681116785e798a9f681d3fb7f98dba54267b))
* **ci:** deploy to crates.io ([790668e](https://github.com/JerkyTreats/meld/commit/790668ef601d6305adce85c99a4e7bc64fa9603a))
* **cli:** make context generate blocking-only ([e65970a](https://github.com/JerkyTreats/meld/commit/e65970a9af5bdae4d465e267dda88915af5b5c7d))
* **cli:** resolve relative paths ([887320f](https://github.com/JerkyTreats/meld/commit/887320f5737850698048ef0712489f477ee3b66b))
* **diagnostics:** surface generation failures and local auth warnings ([02f5824](https://github.com/JerkyTreats/meld/commit/02f58240afa694fc5d82682857c853c90cb328ae))
* **observability:** add typed summaries and provider test lifecycle events ([d3e12e3](https://github.com/JerkyTreats/meld/commit/d3e12e3c7297cc644864e53c4a41a6872ae55712))
* **observability:** align timestamps and bound command summaries ([b7d8a40](https://github.com/JerkyTreats/meld/commit/b7d8a4046d9c652a962301a43c2bbcc6b3496b6d))
* **observability:** include context-generate path identity fields ([2e9e8f3](https://github.com/JerkyTreats/meld/commit/2e9e8f3c40dbea7157a6d95aeeb45f1029ab7d56))
* **queue:** coalesce queued and in-flight generation duplicates ([cefe949](https://github.com/JerkyTreats/meld/commit/cefe949c7c61c4bfc7258b6373e981e0d4eac928))
* **queue:** emit per-item enqueue events for batch requests ([f9900db](https://github.com/JerkyTreats/meld/commit/f9900dbdc931576e1d3d5da8535b84a7848b9021))
* **scan:** emit batched scan_progress events by node count ([251f9a8](https://github.com/JerkyTreats/meld/commit/251f9a8347f817b270d81631e4f5a0ea6e5c72de))
* **xdg:** preserve agent entries and surface config validation errors ([1ce8cd9](https://github.com/JerkyTreats/meld/commit/1ce8cd9d85268d86563036643155505028a77b15))
