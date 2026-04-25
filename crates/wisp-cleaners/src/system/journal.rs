//! `arch.journal` – systemd journal vacuum cleaner.

use wisp_core::types::{
    CleanAction, CleanerGroup, CleanerId, CleanerMeta, ExternalCmd, RiskLevel,
};
use wisp_platform::{Distro, DistroKind};

use crate::{CleanCtx, CleanerEntry, PlanFuture, CLEANERS};

struct JournalMeta;

impl CleanerMeta for JournalMeta {
    fn id(&self) -> CleanerId { CleanerId::new("arch.journal") }
    fn name(&self) -> &str { "Systemd journal" }
    fn description(&self) -> &str {
        "Vacuum systemd journal to a maximum of 500 MB, keeping entries from the last 2 weeks."
    }
    fn risk(&self) -> RiskLevel { RiskLevel::Safe }
    fn requires_root(&self) -> bool { true }
    fn supported_on(&self, distro: &dyn Distro) -> bool {
        distro.kind() == DistroKind::Arch
    }
    fn group(&self) -> CleanerGroup { CleanerGroup::System }
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        // Estimate current journal disk usage
        let estimated = journal_disk_usage().await;

        Ok(vec![CleanAction::RunExternal {
            cmd: ExternalCmd {
                program: "journalctl".into(),
                args: vec![
                    "--vacuum-size=500M".into(),
                    "--vacuum-time=2weeks".into(),
                ],
            },
            estimated_size: estimated,
        }])
    })
}

async fn journal_disk_usage() -> Option<u64> {
    let out = tokio::process::Command::new("journalctl")
        .arg("--disk-usage")
        .output()
        .await
        .ok()?;

    // Output: "Archived and active journals take up 123.4 M in the file system."
    let text = String::from_utf8_lossy(&out.stdout);
    parse_disk_usage(&text)
}

fn parse_disk_usage(text: &str) -> Option<u64> {
    // Simple heuristic: find the size token before "in the file system"
    let part = text.split("in the file system").next()?;
    let token = part.split_whitespace().rev().find(|s| {
        s.chars().next().is_some_and(|c| c.is_ascii_digit())
    })?;
    let unit = part.split_whitespace().rev().find(|s| {
        matches!(*s, "B" | "K" | "M" | "G" | "T")
    });

    let value: f64 = token.replace(',', ".").parse().ok()?;
    let multiplier: u64 = match unit.unwrap_or("B") {
        "K" => 1_024,
        "M" => 1_024 * 1_024,
        "G" => 1_024 * 1_024 * 1_024,
        _ => 1,
    };
    Some((value * multiplier as f64) as u64)
}

static META: JournalMeta = JournalMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };
