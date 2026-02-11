# Non-Aligned Piece-Local Scheduling Refactor Plan

## Status Snapshot (2026-02-10)
### Completed in current branch
1. Added piece-local request API in `PieceManager`:
- `requestable_block_addresses_for_piece(piece_index)`.
2. Routed `AssignWork` request generation through `PieceManager` API:
- pending piece loop
- newly selected piece loop
3. Routed `BulkCancel` tuple generation through piece-local API:
- `cancel_tuples_for_piece(piece_index)`.
4. Kept non-aligned guard behavior:
- global block-bitfield suppression is not used for non-aligned piece grids.
5. Added/updated regression tests:
- non-aligned suppression regression in state
- PM unit tests for requestable addresses (aligned/non-aligned/assembler mask)
- request/cancel identity integration tests (aligned + non-aligned)
6. Full suite validation:
- `cargo test` passes end-to-end outside sandbox constraints.

## Objective
Refactor scheduler/block-query flow so `AssignWork` no longer depends on global `block_bitfield` checks directly, and instead relies on piece-local APIs. Preserve aligned-path behavior and avoid introducing duplicate completion authority.

This remains a long-term architectural cleanup on top of the immediate bug fix.

## Current Problem
For non-aligned piece sizes, global 16KiB slots can overlap adjacent pieces.  
Directly using global bitmap suppression in scheduler can drop required piece-local boundary requests.  
The current fix addresses immediate behavior, but scheduler still owns too much block-level decision logic.

## Target Architecture
1. `PieceManager` is lifecycle authority:
- `Need/Pending/Done`
- piece completion semantics
- piece-level "what is still requestable" answers

2. `BlockManager` is geometry/addressing authority:
- piece-local block address generation
- low-level block status storage (transitional: includes global bitmap)
- no piece lifecycle ownership

3. `TorrentState` orchestrates only:
- peer/pipeline/interest/choke flow
- calls piece-local APIs instead of doing block math directly

## Scope
### In scope
1. Move scheduler requestability decisions behind PM/BM piece-local APIs.
2. Remove direct global bitmap checks from `AssignWork`.
3. Keep aligned fast-path optimizations inside PM/BM internals (optional).
4. Keep `BulkCancel` tuple generation piece-local.
5. Define staged retirement path for global `block_bitfield` as a decision source.

### Out of scope
1. Reworking wire protocol.
2. Reworking disk IO manager.
3. New completion authority fields in `BlockManager`.
4. Broad performance rewrite.

## Phased Implementation

### Phase 0: Baseline and guardrails (done)
1. Record baseline green tests:
- non-aligned regressions
- request/cancel identity integration tests
- tiny-piece tests
- aligned sanity integration test
2. Freeze test fixtures for deterministic replay.

### Phase 1: API extraction (done)
1. Add PM-facing piece-local query API:
- `requestable_blocks_for_piece(...)`
2. Keep current `AssignWork` path, but add a test-only comparator:
- old path tuples vs new API tuples on aligned cases.

### Phase 2: Switch scheduler (done)
1. Replace `AssignWork` block iteration with PM API in:
- pending piece loop
- newly-selected piece loop
2. Preserve:
- pipeline depth
- active block dedupe
- assembler mask filtering
- v2 clamping
- endgame logic

### Phase 3: Immediate cleanup (done)
1. Remove obsolete direct block math from `state.rs`.
2. Keep cancel path fully piece-local via shared helper.
3. Remove any dead helpers exposed only for old path.

### Phase 4: Global `block_bitfield` retirement (planned)
Goal: remove global bitmap as a completion/requestability decision authority while preserving behavior and performance.

1. Inventory every callsite that reads global bitmap for decisions.
- Categorize as:
  - request scheduling
  - duplicate suppression
  - metrics/telemetry only
2. Replace remaining decision callsites with piece-local APIs.
- Add PM APIs as needed for:
  - block completion checks scoped to a piece
  - piece-local duplicate suppression decisions
3. Restrict global bitmap usage to transitional non-authoritative roles.
- read-only for diagnostics/metrics if still needed
- no scheduling/completion gating decisions
4. Introduce deprecation boundary in code comments + module docs.
- explicit note: global bitmap is legacy cache, not source of truth
5. Remove global bitmap decision helpers once no callsites remain.

### Phase 5: Optional physical removal (planned, conditional)
1. If no production/telemetry dependency remains:
- remove global bitmap field/storage and related mutation paths.
2. If retained for perf telemetry:
- keep as derived cache only, validated against piece-local truth in tests.

## Regression Strategy (only add tests where needed)
We already have strong targeted coverage. Add tests only where a refactor risk is not already asserted.

### Existing tests to rely on first
1. Non-aligned unit/state suite (`non_aligned` filter).
2. Request identity integration (aligned + non-aligned).
3. Cancel identity integration (aligned + non-aligned).
4. Tiny-piece state/integration tests.
5. One aligned integration sanity test (`test_case_06_rarest_first_strategy`).

### Add tests only if needed
Add only when a refactor delta is not covered by existing tests:
1. Aligned parity comparator test:
- old scheduler tuple list == new API tuple list (test-only harness).
2. Endgame parity test:
- same candidate/request ordering and dedupe behavior.
3. v2 clamp parity test:
- request lengths unchanged under PM API route.
4. Global-bitmap retirement parity tests:
- no piece requestability decision depends on global bitmap state.
- derived cache (if retained) cannot cause request suppression.

If existing tests already catch the behavior, skip adding new tests.

## Acceptance Criteria
1. `AssignWork` has no direct `block_bitfield` decision logic in `state.rs`.
2. All request/cancel tuple generation is piece-local path.
3. No duplicate completion state introduced in `BlockManager`.
4. Existing regression suites pass.
5. Any newly added tests are justified by uncovered risk only.
6. For retirement phase:
- no scheduler/completion gating logic reads global `block_bitfield`.
- either global bitmap is removed, or clearly marked derived/non-authoritative.

## Risk Register and Mitigations
1. **Aligned regression risk**  
Mitigation: aligned request/cancel identity + parity comparator (if needed).

2. **Performance risk on aligned path**  
Mitigation: keep aligned fast-path in PM/BM internals; benchmark only if regression observed.

3. **Behavior drift in endgame/pipeline**  
Mitigation: keep scheduler policy untouched; refactor only block requestability source.

4. **Hidden v2 interaction drift**  
Mitigation: preserve existing clamp logic and verify with current v2 tests before adding new ones.

5. **Retirement refactor overreach risk**  
Mitigation: split migration into callsite batches; require green targeted suite after each batch before next.

## Execution Checklist
1. Confirm baseline test set.
2. Implement PM/BM API extraction. (done)
3. Switch `AssignWork` to PM API. (done)
4. Run targeted suites. (done)
5. Add tests only for uncovered deltas. (done for current bug class)
6. Cleanup dead code. (done for immediate path)
7. Final regression run and review. (done)
8. Start retirement phase:
- map remaining global bitmap decision callsites
- replace in batches with piece-local APIs
- run targeted + full suite each batch
9. Decide end state:
- remove global bitmap completely, or
- retain as derived cache only with explicit invariants/tests

## Suggested Phased Rollout
### PR 1: Callsite audit + guardrails
1. Inventory all global `block_bitfield` decision reads and classify by purpose.
2. Add explicit comments/invariants that piece-local APIs are authoritative for requestability.
3. Run targeted suites:
- non-aligned, request/cancel identity, tiny-piece, aligned sanity.

### PR 2: Replace remaining scheduling/completion decision reads
1. Migrate one callsite batch at a time to piece-local APIs.
2. Keep behavior parity by preserving ordering/pipeline/endgame policies.
3. Run targeted suites after each batch; run full `cargo test` at PR end.

### PR 3: Deprecate or remove global bitmap authority
1. Remove dead decision helpers and callsites.
2. Either:
- fully remove global bitmap storage, or
- keep as derived cache for metrics only (non-authoritative).
3. Run full suite and confirm no requestability/completion gates read global bitmap.

## Notes
This plan intentionally prefers incremental migration and behavior parity over broad redesign.  
The core rule is: piece-local questions must be answered through piece-local APIs.
