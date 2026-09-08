//! Agent self-install/repair — spec §9.3 (Task 22). Placeholder until
//! Task 22 lands.

use super::{Category, PassReport, SelfHealConfig};
use crate::context::Context;
use anyhow::Result;
use rusqlite::Connection;

pub fn run(_ctx: &Context, _conn: &Connection, _cfg: &SelfHealConfig, _report: &mut PassReport) -> Result<()> {
    let _ = Category::Agent;
    Ok(())
}
