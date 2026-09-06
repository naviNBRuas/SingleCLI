//! the deterministic scheduler (spec §4): a single `tick()` on goal
//! submitted / task finished / timer / daemon start. reconcile → ready-set
//! → capacity → admit → budget → retry → integrator. always in charge;
//! the LLM brain is only ever consulted for plan / supervise / integrate.
//!
//! the pure core (`tick_pure`) takes a graph + a fake capacity map + a
//! budget and returns a list of actions, with no db / subprocess / LLM, so
//! it is exhaustively unit-testable (plan Task 7). the db + dispatch shell
//! lands in plan Task 8.
