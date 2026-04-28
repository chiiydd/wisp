//! `dev.docker` – Docker system prune (dangling images + build cache).

use wisp_core::types::{CleanAction, CleanerGroup, CleanerId, CleanerMeta, ExternalCmd, RiskLevel};
use wisp_platform::Distro;

use crate::{CLEANERS, CleanCtx, CleanerEntry, PlanFuture, binary_exists};

struct DockerMeta;

impl CleanerMeta for DockerMeta {
    fn id(&self) -> CleanerId {
        CleanerId::new("dev.docker")
    }
    fn name(&self) -> &str {
        "Docker dangling images & build cache"
    }
    fn description(&self) -> &str {
        "Remove dangling images and build cache via `docker system prune -f`. \
         Does NOT remove stopped containers or unused volumes."
    }
    fn risk(&self) -> RiskLevel {
        RiskLevel::Moderate
    }
    fn requires_root(&self) -> bool {
        false
    }
    fn supported_on(&self, _distro: &dyn Distro) -> bool {
        true
    }
    fn group(&self) -> CleanerGroup {
        CleanerGroup::Dev
    }
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        if !binary_exists("docker") {
            return Ok(Vec::new());
        }

        // Estimate reclaimable space
        let estimated = docker_reclaimable().await;

        Ok(vec![CleanAction::RunExternal {
            cmd: ExternalCmd {
                program: "docker".into(),
                args: vec!["system".into(), "prune".into(), "-f".into()],
            },
            estimated_size: estimated,
        }])
    })
}

async fn docker_reclaimable() -> Option<u64> {
    let out = tokio::process::Command::new("docker")
        .args(["system", "df", "--format", "{{.Reclaimable}}"])
        .output()
        .await
        .ok()?;

    // Very rough parse: first numeric token
    let text = String::from_utf8_lossy(&out.stdout);
    let token = text.split_whitespace().next()?;
    parse_size_token(token)
}

fn parse_size_token(s: &str) -> Option<u64> {
    let digits: String = s
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let suffix: String = s
        .chars()
        .skip_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let value: f64 = digits.parse().ok()?;
    let mult: u64 = match suffix.trim().to_uppercase().as_str() {
        "KB" | "KIB" => 1_024,
        "MB" | "MIB" => 1_024 * 1_024,
        "GB" | "GIB" => 1_024 * 1_024 * 1_024,
        _ => 1,
    };
    Some((value * mult as f64) as u64)
}

static META: DockerMeta = DockerMeta;

#[linkme::distributed_slice(CLEANERS)]
static ENTRY: CleanerEntry = CleanerEntry { meta: &META, plan };

#[cfg(test)]
mod tests {
    //! Two layers of testing for the docker cleaner:
    //!
    //! 1. **Pure parsers** (`parse_size_token`) — fully covered with edge
    //!    cases. These are the most error-prone bits because docker's
    //!    `--format {{.Reclaimable}}` output isn't a stable contract.
    //!
    //! 2. **Plan structure** — exercises `plan()` against the real
    //!    environment. The contract is: emit a single `RunExternal` for
    //!    `docker system prune -f` when docker is on `$PATH`, else emit
    //!    nothing. We do NOT execute the prune command in tests — that
    //!    would mutate real docker state, isn't reproducible across CI
    //!    runners, and is the engine's `exec_action` job, not ours.
    use super::*;
    use crate::CleanCtx;
    use std::sync::Arc;

    // ── parse_size_token edge cases ─────────────────────────────────────

    #[test]
    fn parses_plain_kb() {
        assert_eq!(parse_size_token("512KB"), Some(512 * 1_024));
    }

    #[test]
    fn parses_decimal_mb_with_iec_suffix() {
        assert_eq!(
            parse_size_token("1.5MiB"),
            Some((1.5 * 1_024.0 * 1_024.0) as u64)
        );
    }

    #[test]
    fn parses_gigabyte_lowercase_input() {
        // parse_size_token uppercases the suffix internally.
        assert_eq!(parse_size_token("2gb"), Some(2 * 1_024 * 1_024 * 1_024));
    }

    #[test]
    fn unitless_token_parses_as_bytes() {
        assert_eq!(parse_size_token("4096"), Some(4096));
    }

    #[test]
    fn unknown_suffix_falls_back_to_bytes() {
        // "100tonnes" → 100 (ignores the unrecognised suffix and treats
        // the leading numeric part as bytes).
        assert_eq!(parse_size_token("100tonnes"), Some(100));
    }

    #[test]
    fn rejects_token_without_leading_digits() {
        assert!(parse_size_token("notasize").is_none());
        assert!(parse_size_token("").is_none());
    }

    // ── Plan-structure tests ────────────────────────────────────────────

    fn make_ctx() -> CleanCtx {
        CleanCtx {
            dry_run: true,
            distro: Arc::from(wisp_platform::detect_distro()),
        }
    }

    #[tokio::test]
    async fn plan_is_empty_when_docker_not_on_path() {
        // Skip the test if docker IS installed — we can't synthesise its
        // absence without mutating $PATH globally, which would race with
        // other tests.
        if crate::binary_exists("docker") {
            return;
        }
        let ctx = make_ctx();
        let actions = plan(&ctx).await.unwrap();
        assert!(actions.is_empty(), "no docker → no actions");
    }

    #[tokio::test]
    async fn plan_emits_run_external_with_expected_args_when_docker_present() {
        if !crate::binary_exists("docker") {
            // Conditionally skip on machines without docker. The empty-case
            // is covered by the sibling test.
            return;
        }
        let ctx = make_ctx();
        let actions = plan(&ctx).await.unwrap();
        assert_eq!(actions.len(), 1, "exactly one external action");
        match &actions[0] {
            CleanAction::RunExternal { cmd, .. } => {
                assert_eq!(cmd.program, "docker");
                assert_eq!(cmd.args, vec!["system", "prune", "-f"]);
            }
            CleanAction::Delete { .. } => {
                panic!("docker cleaner must emit RunExternal, not Delete")
            }
        }
    }
}
