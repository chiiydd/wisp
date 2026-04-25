//! `dev.docker` – Docker system prune (dangling images + build cache).

use wisp_core::types::{
    CleanAction, CleanerGroup, CleanerId, CleanerMeta, ExternalCmd, RiskLevel,
};
use wisp_platform::Distro;

use crate::{CleanCtx, CleanerEntry, PlanFuture, CLEANERS};

struct DockerMeta;

impl CleanerMeta for DockerMeta {
    fn id(&self) -> CleanerId { CleanerId::new("dev.docker") }
    fn name(&self) -> &str { "Docker dangling images & build cache" }
    fn description(&self) -> &str {
        "Remove dangling images and build cache via `docker system prune -f`. \
         Does NOT remove stopped containers or unused volumes."
    }
    fn risk(&self) -> RiskLevel { RiskLevel::Moderate }
    fn requires_root(&self) -> bool { false }
    fn supported_on(&self, _distro: &dyn Distro) -> bool { true }
    fn group(&self) -> CleanerGroup { CleanerGroup::Dev }
}

fn plan<'a>(_ctx: &'a CleanCtx) -> PlanFuture<'a> {
    Box::pin(async move {
        let exists = tokio::process::Command::new("which")
            .arg("docker")
            .output()
            .await
            .map(|o| o.status.success())
            .unwrap_or(false);

        if !exists {
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
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
    let suffix: String = s.chars().skip_while(|c| c.is_ascii_digit() || *c == '.').collect();
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
