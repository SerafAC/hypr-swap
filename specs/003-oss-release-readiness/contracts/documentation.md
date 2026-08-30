# Contract: The document map

FR-084 is the rule this page exists to enforce: **every question a reader may have is answered
authoritatively in exactly one document**, and the others link to it. The `docs-map` check
([ci.md](./ci.md)) holds the structure; this table is what it holds it to.

## Who each document is for

| Document | Reader | Answers |
|---|---|---|
| `README.md` | an end user deciding whether to install | what it is, what it is for, what it requires, how to install, how to configure, how to use (FR-067) |
| `DEVELOPMENT.md` | a developer getting productive | development requirements, setup, running, every test tier, architecture and tree (FR-072) |
| `docs/src/user/` | a user in depth | every setting, every key, every failure mode (FR-079–FR-081) |
| `docs/src/dev/` | a contributor in depth | architecture, workflow, contracts, tiers, releasing (FR-082) |
| `CONTRIBUTING.md` | someone about to open a change | the rules, the spec-driven flow, what review looks for (FR-094) |
| `SECURITY.md` | someone with a vulnerability | private channel, acknowledgement time, supported versions (FR-119, FR-120) |
| `CHANGELOG.md` | a user upgrading | what changed, and what broke (FR-102) |
| `THIRD-PARTY.md` | a packager or reviewer | what ships inside the tree from elsewhere (FR-063) |
| `specs/**` | anyone asking *why*, or *what exactly was promised* | requirements and external contracts — authoritative, and linked to rather than restated (FR-084a, FR-084b) |

## One question, one home

| Question | Authoritative | Everyone else |
|---|---|---|
| What does it do, and should I install it? | `README.md` | site front page links to it |
| What does it require? | `README.md` requirements section | site install page links |
| How do I install it? | site `user/install.md` (every channel) | README carries the short path and links |
| Which keys do I bind, and why `bind` not `binde`? | site `user/binds.md` | README, `--help` |
| What can I put in the configuration file? | `specs/00{1,2}/contracts/config.md`, **included** into site `user/configuration.md` | README shows the common few and links |
| What are the style values, ranges and defaults? | `specs/002/contracts/style-values.md`, **included** into site `user/styling.md` | README links |
| Why is my icon wrong? | site `user/icons.md` | troubleshooting links |
| Where is the daemon's output, and what does it mean? | site `user/troubleshooting.md` | `SECURITY.md`, issue form link |
| How do I build and test it? | `DEVELOPMENT.md` | site `dev/testing.md` expands; README does **not** (FR-068) |
| How is it put together? | `DEVELOPMENT.md` (the seam and the tree) and site `dev/architecture.md` (in full) | `CLAUDE.md` for agents |
| How do I contribute a change? | `CONTRIBUTING.md` | pull-request template restates only the checklist |
| What is verified by what? | `plan.md` tier tables, `{{#include}}`d into site `dev/verification.md` | `CONTRIBUTING.md` links |
| How is a release cut? | `specs/003/contracts/release.md`, `{{#include}}`d into site `dev/releasing.md` | `CONTRIBUTING.md` links |
| What counts as a breaking change? | `specs/003/contracts/versioning.md` | `CHANGELOG.md` header links |

**Included, not restated** is the mechanism that makes this survive contact with editing: where a
site page and a contract would otherwise say the same thing, the page uses mdBook's
`{{#include}}` so the two are the same bytes ([research.md](../research.md) R32). The settings
catalogue is additionally walked by a unit test against `theme.rs` and `config.rs`, so the
published reference cannot drift from what the program accepts (FR-083).

## The site's shape (FR-077)

```text
User guide                         Developer guide
├── Installing                     ├── Architecture
├── Binding the shortcuts          ├── The spec-driven workflow
├── Configuration                  ├── Testing
├── Appearance and themes          ├── Verification coverage
├── Program icons                  └── Releasing
└── Troubleshooting
```

Each part is navigable without reading the other. The front page states which release the site
documents and marks anything on the default branch that is not yet released; there are no
per-release snapshots (FR-078a).

## What the README may not contain (FR-068)

No build-from-source-for-development instructions, no test commands, no architecture, no
contribution mechanics. It links to `DEVELOPMENT.md` and `CONTRIBUTING.md` instead. The
`docs-map` check greps for the obvious violations (`cargo test`, `cargo clippy`, "architecture"
as a heading) so this cannot creep back in.
