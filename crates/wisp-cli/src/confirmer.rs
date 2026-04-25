//! CLI confirmer implementations.

use std::future::Future;
use std::io::{self, Write};
use std::pin::Pin;

use wisp_core::types::{Confirmation, ConfirmRequest, RiskLevel};

// ─── Auto-approve confirmer (-y flag) ────────────────────────────────────────

/// Approves everything up to `Moderate`.  Dangerous actions are always denied
/// unless `approve_dangerous` is also set.
pub struct AutoConfirmer {
    pub approve_dangerous: bool,
}

impl wisp_core::types::Confirmer for AutoConfirmer {
    fn ask<'a>(&'a self, req: ConfirmRequest) -> Pin<Box<dyn Future<Output = Confirmation> + Send + 'a>> {
        let decision = if req.risk == RiskLevel::Dangerous && !self.approve_dangerous {
            Confirmation::Denied
        } else {
            Confirmation::ApprovedAll
        };
        Box::pin(std::future::ready(decision))
    }
}

// ─── Interactive confirmer (human output) ─────────────────────────────────────

/// Prompts the user via stdin/stdout.
pub struct CliConfirmer;

impl wisp_core::types::Confirmer for CliConfirmer {
    fn ask<'a>(&'a self, req: ConfirmRequest) -> Pin<Box<dyn Future<Output = Confirmation> + Send + 'a>> {
        Box::pin(async move { prompt(&req) })
    }
}

fn prompt(req: &ConfirmRequest) -> Confirmation {
    if req.risk == RiskLevel::Dangerous {
        print!(
            "\n  ⚠  DANGEROUS action – type 'yes' to confirm: "
        );
        let _ = io::stdout().flush();
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            return Confirmation::Denied;
        }
        if input.trim() == "yes" {
            Confirmation::Approved
        } else {
            Confirmation::Denied
        }
    } else {
        print!("  Proceed? [Y/n/a(ll)] ");
        let _ = io::stdout().flush();
        let mut input = String::new();
        if io::stdin().read_line(&mut input).is_err() {
            return Confirmation::Denied;
        }
        match input.trim().to_lowercase().as_str() {
            "" | "y" | "yes" => Confirmation::Approved,
            "a" | "all" => Confirmation::ApprovedAll,
            _ => Confirmation::Denied,
        }
    }
}
