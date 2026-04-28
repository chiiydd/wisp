//! `arch.journal` – systemd journal vacuum cleaner.

use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, ExternalCmd, RiskLevel};
use wisp_platform::{Distro, DistroKind};

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture};

struct JournalMeta;

impl CleanerMeta for JournalMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("arch.journal")
    }
    fn name(&self) -> &str {
        "Systemd journal"
    }
    fn description(&self) -> &str {
        "Vacuum systemd journal to a maximum of 500 MB, keeping entries from the last 2 weeks."
    }
    fn risk(&self) -> RiskLevel {
        RiskLevel::Safe
    }
    fn requires_root(&self) -> bool {
        true
    }
    fn supported_on(&self, distro: &dyn Distro) -> bool {
        distro.kind() == DistroKind::Arch
    }
    fn group(&self) -> CleanerGroup {
        CleanerGroup::System
    }
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        // Estimate current journal disk usage
        let estimated = journal_disk_usage().await;

        Ok(vec![CleanAction::RunExternal {
            cmd: ExternalCmd {
                program: "journalctl".into(),
                args: vec!["--vacuum-size=500M".into(), "--vacuum-time=2weeks".into()],
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
    let token = part
        .split_whitespace()
        .rev()
        .find(|s| s.chars().next().is_some_and(|c| c.is_ascii_digit()))?;
    let unit = part
        .split_whitespace()
        .rev()
        .find(|s| matches!(*s, "B" | "K" | "M" | "G" | "T"));

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_megabyte_journal_size() {
        let s = "Archived and active journals take up 123.4 M in the file system.";
        let parsed = parse_disk_usage(s).expect("size should parse");
        assert_eq!(parsed, (123.4 * 1_024.0 * 1_024.0) as u64);
    }

    #[test]
    fn parses_gigabyte_journal_size() {
        let s = "Archived and active journals take up 2.0 G in the file system.";
        let parsed = parse_disk_usage(s).expect("size should parse");
        assert_eq!(parsed, 2 * 1_024 * 1_024 * 1_024);
    }

    #[test]
    fn parses_kilobyte_with_comma_decimal() {
        // Some locales emit "8,2 K" instead of "8.2 K"
        let s = "Archived and active journals take up 8,2 K in the file system.";
        let parsed = parse_disk_usage(s).expect("size should parse");
        assert_eq!(parsed, (8.2 * 1_024.0) as u64);
    }

    #[test]
    fn parses_bytes_when_no_unit_marker() {
        let s = "Archived and active journals take up 512 in the file system.";
        let parsed = parse_disk_usage(s).expect("size should parse");
        assert_eq!(parsed, 512);
    }

    #[test]
    fn returns_none_on_garbage_input() {
        assert!(parse_disk_usage("nothing relevant here").is_none());
        assert!(parse_disk_usage("").is_none());
    }
}
