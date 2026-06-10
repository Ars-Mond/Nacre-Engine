<!--
SYNC IMPACT REPORT
==================
Version change: 1.1.0 → 1.2.0
Bump rationale: MINOR — three new non-negotiable principles added. Additive; no
existing principle (I–V) is changed or removed.

Added principles:
  - VI.   Structured Logging Discipline
  - VII.  Phase Commit Discipline
  - VIII. Safe Code Discipline

Consolidated:
  - The "Phase Commits" subsection added under Development Workflow in v1.1.0 is
    promoted to Principle VII and removed from Development Workflow to avoid
    duplicating its normative text.

Principles I–V: unchanged.
Removed sections: none (the Phase Commits subsection was promoted, not dropped).

Templates checked for consistency:
  - .specify/templates/plan-template.md ............ ✅ compatible (Constitution Check is dynamic)
  - .specify/templates/spec-template.md ............ ✅ compatible (no constitution-bound content)
  - .specify/templates/tasks-template.md ........... ✅ compatible (no constitution-bound content)

Follow-up (implementation work, not part of this amendment):
  - Add the `log` facade; replace silent light-cap clamping with a `warn!`; enforce
    clippy print_stdout/print_stderr/dbg_macro/undocumented_unsafe_blocks = deny for
    the library; review `.expect()` sites against Principle VIII.

Deferred / TODO placeholders: none
-->

# NacreEngine Constitution

NacreEngine is a minimal real-time PBR engine/renderer written in Rust on top of
`wgpu`. It implements physically based lighting (metallic-roughness workflow,
Cook-Torrance BRDF) and ships as a library crate that embeds into any wgpu
application and into `iced`. This constitution defines the non-negotiable
principles that govern its design, implementation, and evolution.

## Core Principles

### I. Pure Rust, Zero Native Dependencies

The engine MUST build with `cargo` alone on a clean **stable** toolchain, with no
system packages installed.

- **Forbidden**: any dependency (including transitive) that requires system C/C++
  libraries, headers, or a native toolchain to **build** — e.g. `cc`, `cmake`,
  `pkg-config`, or vendored C/C++ sources.
- **Allowed**: transitive pure-Rust `*-sys`/FFI crates that compile on the stable
  toolchain without system packages and only **load OS-provided libraries at
  runtime** (GPU drivers: Vulkan/Metal/D3D12/OpenGL; RenderDoc; and the like).
- Direct use of native APIs in engine code is forbidden — all GPU access goes
  through the `wgpu` API only.
- No build step may require a system package manager, a pre-installed SDK, or any
  non-Rust compiler; adding the engine to a project MUST be nothing more than
  `cargo add`.

**Rationale**: The goal is a reproducible cross-platform `cargo build` on clean
stable, free of "works on my machine" failures from missing system libraries.
Runtime FFI to GPU drivers is unavoidable by design and is encapsulated inside
`wgpu` — forbidding it would forbid GPU rendering itself. What matters is that no
**build** step needs a system package or a C/C++ toolchain.

### II. Cross-Platform Parity (Windows, Linux, macOS)

The engine MUST behave identically on Windows, Linux, and macOS.

- Observable behavior (rendering output, public API semantics) MUST be the same on
  all three operating systems.
- Platform-specific code is forbidden outside explicit, documented abstractions.
- CI MUST build and run the full test suite on all three operating systems. A
  failure on any one of them blocks the merge.

**Rationale**: The engine is a portable library. Consumers must be able to rely on
the same physically based result on every supported desktop OS, with no hidden
platform branches to reason about.

### III. Library, Not I/O Owner

The engine MUST NEVER own input/output or device lifecycle.

- It MUST NEVER create a window, surface, event loop, or wgpu `Device`/`Queue`.
- It receives the `Device`, the `Queue`, and a target texture (its format and
  size) from the host application.
- It MUST embed equally well into a raw wgpu application and into `iced` (via the
  shader widget / `Primitive`).

**Rationale**: Ownership of the window, the event loop, and the GPU device belongs
to the host application. By accepting these from the outside, the engine stays
embeddable, composable, and free of assumptions about its runtime environment.

### IV. Minimal Explicit Dependencies

The engine MUST keep both its API surface and its dependency set small.

- The public API MUST be as small as the feature set allows.
- Every dependency MUST be explicitly justified in the pull request that
  introduces it.
- Pure-Rust crates are preferred. The baseline dependency set is
  `std` + `wgpu` + `glam` (math) + `bytemuck` (GPU data packing); additions beyond
  this baseline carry the burden of proof.

**Rationale**: A small surface and a small dependency set mean faster builds, a
smaller maintenance and security footprint, and fewer breaking changes forced by
upstream churn.

### V. Integration First, the API Is a Contract

Integration is the primary deliverable; the public API is a contract.

- The `update → prepare → render` lifecycle is stable and MUST remain stable.
- Breaking changes to the public API ship ONLY through a major version bump under
  Semantic Versioning.
- Deprecations MUST be announced before removal and documented in the changelog.

**Rationale**: Consumers build their render loop on top of the engine's lifecycle.
Stability of that contract is the product; unannounced breakage would betray every
integrator at once.

### VI. Structured Logging Discipline

All logging MUST go through the `log` crate facade (`error!`, `warn!`, `info!`,
`debug!`, `trace!`).

- `println!`, `eprintln!`, and `dbg!` are FORBIDDEN in committed code. Exception:
  `examples/`, where stdout output is part of the demonstration.
- Log messages MUST be in English and self-sufficient for diagnosis from a single
  log file: they include context (the operation, the file path / identifier, counts,
  sizes).
- Swallowing errors is forbidden: code that catches an error MUST either propagate it
  or log it via `warn!`/`error!` with context. Silent `let _ = ...` and empty `Err`
  arms are violations.
- Enforcement: the clippy lints `print_stdout`, `print_stderr`, and `dbg_macro` are
  set to `deny` for library code.

**Rationale**: One structured, leveled log stream makes failures diagnosable in the
field without a debugger; ad-hoc prints and silently dropped errors hide problems
from the host application that embeds the engine.

### VII. Phase Commit Discipline

Spec Kit work MUST be committed phase by phase.

- After completing **each** Spec Kit phase (`specify`, `clarify`, `plan`, `tasks`,
  `implement`), a git commit MUST be made via `/speckit-git-commit` before moving to
  the next phase.
- Commit messages are written in English and are concise: the phase plus the
  artifacts touched.
- One commit equals one phase; changes from different phases MUST NOT be mixed.
  Mixing is allowed only when a phase produced no changes.

**Rationale**: Phase-aligned commits keep history legible and reviewable, make each
Spec Kit step independently revertible, and stop unrelated changes from riding along.

### VIII. Safe Code Discipline

Committed code MUST avoid panics and undisciplined `unsafe`.

- `.unwrap()` is FORBIDDEN in committed code; `.expect()` is allowed ONLY with a
  message explaining why the failure is impossible (an invariant). Errors are handled
  through `Result`/`Option`, the `?` operator, and meaningful error types. Exception:
  `#[cfg(test)]`, tests, and `examples/`.
- `unsafe` is permitted ONLY when its necessity is proven, and STRICTLY by Rust
  community conventions:
  - the smallest possible block scope;
  - a mandatory `// SAFETY:` comment justifying the invariants above every `unsafe`
    block;
  - `unsafe` encapsulated behind a safe public API;
  - the clippy lint `undocumented_unsafe_blocks` set to `deny`.

**Rationale**: Panics turn recoverable conditions into crashes for the embedding host;
disciplined, documented `unsafe` keeps memory safety auditable.

## Technology & Language Constraints

**Shaders**: WGSL only. Hand-written GLSL/HLSL/MSL targeting individual backends is
forbidden. A single WGSL source MUST port to every wgpu backend (DX12, Vulkan,
Metal, GLES).

**Core libraries**: Math is provided by `glam`; CPU↔GPU data packing is provided by
`bytemuck`. Rendering is provided by `wgpu`.

**Toolchain**: Stable Rust only. Nightly-only features are forbidden. No dependency
may require installing system packages (see Principle I).

**Working language**:

- All code, comments, identifiers, documentation, commit messages, and issues MUST
  be written in **English**.
- Team and AI collaboration — task discussion, design conversation, review
  dialogue, and chat — is conducted in **Russian**.

## Development Workflow & Quality Gates

Every pull request MUST satisfy the following gates before it can merge:

1. **Cross-platform CI green** — builds and tests pass on Windows, Linux, and
   macOS (Principle II).
2. **Pure-Rust build check** — no new dependency requires system C/C++ libraries,
   headers, or a native toolchain to build; `cargo build` stays clean on stable with
   no system packages (Principle I). Pure-Rust transitive `*-sys`/FFI crates that only
   load OS libraries at runtime are allowed.
3. **Dependency justification** — any new dependency is justified in the PR
   description (Principle IV).
4. **API contract review** — public API breaking changes are flagged, require a
   major version bump, and are recorded in the changelog (Principle V).
5. **Lint & format clean** — `cargo fmt --check` and `cargo clippy` (warnings
   denied) pass.
6. **Tests** — the suite runs under `cargo test` and passes on all platforms.

Reviewers MUST verify constitution compliance as part of every review. Any
deviation MUST be either removed or explicitly justified (and recorded) before
merge; unjustified violations are grounds for rejection. Phase-by-phase commits are
governed by Principle VII.

## Governance

This constitution supersedes all other development practices. When any other
document, convention, or habit conflicts with it, this constitution wins.

**Versioning**: This constitution is versioned with Semantic Versioning
(MAJOR.MINOR.PATCH):

- **MAJOR** — backward-incompatible governance changes: a principle is removed or
  redefined in a way that invalidates prior compliance.
- **MINOR** — a new principle or section is added, or existing guidance is
  materially expanded.
- **PATCH** — clarifications, wording, and typo fixes that do not change meaning.

**Amendment procedure**:

1. Propose the change via a pull request that edits this file.
2. The PR MUST include a rationale, the resulting version bump, and an updated Sync
   Impact Report at the top of the file.
3. Dependent templates and guidance documents (`.specify/templates/*`) MUST be
   reviewed and synchronized in the same PR.
4. Approval of the PR ratifies the amendment; the new version takes effect on
   merge.

**Compliance review**: Every PR and every code review MUST verify adherence to
these principles. Complexity or deviation MUST be justified against the affected
principle; if it cannot be justified, it MUST be removed. Runtime development
guidance for AI agents lives in `CLAUDE.md` and MUST stay consistent with this
constitution.

**Version**: 1.2.0 | **Ratified**: 2026-06-09 | **Last Amended**: 2026-06-10
