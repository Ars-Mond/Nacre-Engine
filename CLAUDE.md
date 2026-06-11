<!-- SPECKIT START -->
## Active feature: 002-tonemap-exposure

Plan: `specs/002-tonemap-exposure/plan.md` (see also `spec.md`, `research.md`,
`data-model.md`, `contracts/`, `quickstart.md`).

**What**: Tone mapping + exposure — map the linear HDR lighting result into
displayable range. Order: `lighting → × exposure → tone curve → sRGB target`; alpha
untouched. Four fixed operators: None (default, = feature 001), Reinhard (`c/(1+c)`),
ACES (Hill fit), Khronos PBR Neutral.

**Key design**: additive only. Settings go through a new `Engine::set_tone_mapping`
setter (default None / exposure 1.0), NOT a `Scene`/`EngineConfig` field — so existing
host code compiles unmodified and the default path is byte-identical to feature 001
(existing goldens stay valid). The `update → prepare → render` contract is unchanged
(Principle V; ships as a SemVer minor). Implemented in `fs_main` + one group-0 uniform
(binding 3); no new pass, target, or dependency.

**Built on**: feature 001 (complete) — the engine, golden pipeline (per-OS + per-pixel ε
+ cross-OS SSIM, `UPDATE_GOLDEN`), both example integrations, and the 3-OS CI.

For full technology, structure, and command details, read the plan above.
<!-- SPECKIT END -->
