//! Coordinator self-correction — spec §9.2 (Task 21). Placeholder until
//! Task 21 lands; `run_pass` already calls this unconditionally so the
//! category toggle and schema are wired end-to-end from Task 20 onward.

use super::{Category, PassReport, SelfHealConfig};
use crate::context::Context;
use anyhow::Result;
use rusqlite::Connection;

pub fn run(_ctx: &Context, _conn: &Connection, _cfg: &SelfHealConfig, _report: &mut PassReport) -> Result<()> {
    let _ = Category::Coordinator; // silences unused-import until Task 21 adds real steps here.
    Ok(())
}
