//! L1 – Platform abstraction layer.
//!
//! Provides traits for OS/distro detection and package manager/init system
//! introspection.  Arch Linux is the first-class implementation; the trait
//! interfaces are intentionally designed for multi-distro extensibility.

pub mod arch;

// ─── Distro trait & detection ─────────────────────────────────────────────────

/// The kind of Linux distribution that has been detected.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DistroKind {
    Arch,
    Debian,
    Fedora,
    OpenSuse,
    Unknown(String),
}

/// Abstraction over a Linux distribution.
pub trait Distro: Send + Sync {
    fn kind(&self) -> DistroKind;
    fn name(&self) -> &str;
    /// The `ID` field from `/etc/os-release`, e.g. `"arch"`.
    fn id(&self) -> &str;
}

/// Abstraction for package manager introspection (query-only).
pub trait PackageManager: Send + Sync {
    fn name(&self) -> &str;
    fn is_available(&self) -> bool;
}

/// Abstraction for the init system.
pub trait InitSystem: Send + Sync {
    fn name(&self) -> &str;
}

// ─── Auto-detection ───────────────────────────────────────────────────────────

/// Detect the running distribution by reading `/etc/os-release`.
///
/// Falls back to an opaque `Unknown` implementation when detection fails.
pub fn detect_distro() -> Box<dyn Distro> {
    if let Ok(content) = std::fs::read_to_string("/etc/os-release") {
        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("ID=") {
                let id = rest.trim_matches('"');
                if id == "arch" {
                    return Box::new(arch::ArchDistro);
                }
            }
        }
    }
    Box::new(UnknownDistro {
        id: "unknown".into(),
        name: "Unknown Linux".into(),
    })
}

// ─── Fallback implementation ──────────────────────────────────────────────────

struct UnknownDistro {
    id: String,
    name: String,
}

impl Distro for UnknownDistro {
    fn kind(&self) -> DistroKind {
        DistroKind::Unknown(self.id.clone())
    }
    fn name(&self) -> &str {
        &self.name
    }
    fn id(&self) -> &str {
        &self.id
    }
}
