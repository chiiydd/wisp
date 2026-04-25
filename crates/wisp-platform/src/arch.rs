//! Arch Linux platform implementation.

use crate::{Distro, DistroKind, InitSystem, PackageManager};

/// Arch Linux distro marker.
pub struct ArchDistro;

/// The `pacman` package manager.
pub struct Pacman;

/// The `systemd` init system (standard on Arch).
pub struct Systemd;

impl Distro for ArchDistro {
    fn kind(&self) -> DistroKind { DistroKind::Arch }
    fn name(&self) -> &str { "Arch Linux" }
    fn id(&self) -> &str { "arch" }
}

impl PackageManager for Pacman {
    fn name(&self) -> &str { "pacman" }
    fn is_available(&self) -> bool {
        std::process::Command::new("pacman")
            .arg("--version")
            .output()
            .is_ok()
    }
}

impl InitSystem for Systemd {
    fn name(&self) -> &str { "systemd" }
}
