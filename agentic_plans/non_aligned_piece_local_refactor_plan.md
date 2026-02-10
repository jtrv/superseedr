# Non-Aligned Piece-Local Scheduling Refactor Plan

## Objective
Refactor scheduler/block-query flow so `AssignWork` no longer depends on global `block_bitfield` checks directly, and instead relies on piece-local APIs. Preserve aligned-path behavior and avoid introducing duplicate completion authority.

This is a long-term architectural cleanup on top of the immediate bug fix.

## Current Problem
For non-aligned piece sizes, global 16KiB slots can overlap adjacent pieces.  
Directly using global bitmap suppression in scheduler can drop required piece-local boundary requests.  
The current fix addresses immediate behavior, but scheduler still owns too much block-level decision logic.

## Target Architecture
1. `PieceManager` is lifecycle authority:
- `Need/Pending/Done`
- piece completion semantics

2. `BlockManager` is geometry/addressing authority:
- piece-local block address generation
- low-level global bitmap storage
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

### Out of scope
1. Reworking wire protocol.
2. Reworking disk IO manager.
3. New completion authority fields in `BlockManager`.
4. Broad performance rewrite.

## Phased Implementation

### Phase 0: Baseline and guardrails
1. Record baseline green tests:
- non-aligned regressions
- request/cancel identity integration tests
- tiny-piece tests
- aligned sanity integration test
2. Freeze test fixtures for deterministic replay.

### Phase 1: API extraction (no behavior change target)
1. Add PM-facing piece-local query API:
- `requestable_blocks_for_piece(...)`
2. Keep current `AssignWork` path, but add a test-only comparator:
- old path tuples vs new API tuples on aligned cases.

### Phase 2: Switch scheduler
1. Replace `AssignWork` block iteration with PM API in:
- pending piece loop
- newly-selected piece loop
2. Preserve:
- pipeline depth
- active block dedupe
- assembler mask filtering
- v2 clamping
- endgame logic

### Phase 3: Cleanup
1. Remove obsolete direct block math from `state.rs`.
2. Keep cancel path fully piece-local via shared helper.
3. Remove any dead helpers exposed only for old path.

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

If existing tests already catch the behavior, skip adding new tests.

## Acceptance Criteria
1. `AssignWork` has no direct `block_bitfield` decision logic in `state.rs`.
2. All request/cancel tuple generation is piece-local path.
3. No duplicate completion state introduced in `BlockManager`.
4. Existing regression suites pass.
5. Any newly added tests are justified by uncovered risk only.

## Risk Register and Mitigations
1. **Aligned regression risk**  
Mitigation: aligned request/cancel identity + parity comparator (if needed).

2. **Performance risk on aligned path**  
Mitigation: keep aligned fast-path in PM/BM internals; benchmark only if regression observed.

3. **Behavior drift in endgame/pipeline**  
Mitigation: keep scheduler policy untouched; refactor only block requestability source.

4. **Hidden v2 interaction drift**  
Mitigation: preserve existing clamp logic and verify with current v2 tests before adding new ones.

## Execution Checklist
1. Confirm baseline test set.
2. Implement PM/BM API extraction.
3. Switch `AssignWork` to PM API.
4. Run targeted suites.
5. Add tests only for uncovered deltas.
6. Cleanup dead code.
7. Final regression run and review.

## Notes
This plan intentionally prefers incremental migration and behavior parity over broad redesign.  
The core rule is: piece-local questions must be answered through piece-local APIs.
