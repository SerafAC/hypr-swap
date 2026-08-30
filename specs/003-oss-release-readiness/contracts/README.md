# Contracts: Open-Source Release Readiness

The external surface this feature defines or extends, and the requirement each page answers.
Features 001 and 002 remain the authority on everything they defined; the pages here that carry a
"(delta)" label state only what changes.

| Contract | Covers | Requirements |
|---|---|---|
| [cli.md](./cli.md) (delta) | `--environment`, the version string's shape | FR-103, FR-104, FR-116 |
| [diagnostics.md](./diagnostics.md) (delta) | `Started`, `Stopping`, `CompositorVersionUnsupported` | FR-112, FR-113, FR-114, FR-118 |
| [versioning.md](./versioning.md) | Semver policy, the stable surface, the changelog form | FR-101, FR-101a, FR-102, FR-102a, FR-117 |
| [release.md](./release.md) | The release workflow: input, preconditions, steps, artefacts, resume | FR-105, FR-106, FR-108, FR-110, FR-111 |
| [packaging.md](./packaging.md) | Install map, dependencies, the verified distribution matrix, the Arch recipe | FR-066, FR-065, FR-107, FR-109, FR-109a |
| [ci.md](./ci.md) | Gating vs informational checks, local reproduction, the E2E image | FR-085–FR-093 |
| [documentation.md](./documentation.md) | The document map: one question, one authoritative answer | FR-067–FR-084b |

## Requirement trace

Requirements not named above are verified rather than contracted — the licensing files, the
contribution documents, and the security policy. Every requirement's verification tier, including
those, is the table in [plan.md](../plan.md), which is itself the deliverable FR-092 asks for.

| Requirement group | Where it is answered |
|---|---|
| FR-062–FR-066a licensing and provenance | `LICENSE`, `THIRD-PARTY.md`, `Cargo.toml`, [packaging.md](./packaging.md), `history-review.md` |
| FR-067–FR-071 the README | [documentation.md](./documentation.md) |
| FR-072–FR-075 the development document | [documentation.md](./documentation.md) |
| FR-076–FR-084b the site | [documentation.md](./documentation.md), [ci.md](./ci.md) |
| FR-085–FR-093 automated verification | [ci.md](./ci.md) |
| FR-094–FR-100 contribution process | `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, the issue forms and pull-request template |
| FR-101–FR-111 versioning and releases | [versioning.md](./versioning.md), [release.md](./release.md), [packaging.md](./packaging.md) |
| FR-112–FR-118 the daemon's own record | [diagnostics.md](./diagnostics.md), [cli.md](./cli.md) |
| FR-119–FR-121 security and maintenance | `SECURITY.md`, `CONTRIBUTING.md`, [ci.md](./ci.md) |
