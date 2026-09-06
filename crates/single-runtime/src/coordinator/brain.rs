//! the LLM brain roles (spec §5): planner (goal → TaskGraph), supervisor
//! (patch the graph on failure), integrator (verify + summarise). each is
//! one `task::run` against a routed agent with a strict "output ONLY JSON"
//! prompt and a first-valid-JSON extractor ported from the Python
//! `single-acp` prototype. no agent is pinned — reasoning routes through
//! the same routing table + pool health as work nodes.
//!
//! fleshed out in plan Tasks 9–10.
