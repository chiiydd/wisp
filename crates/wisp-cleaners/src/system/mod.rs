//! System-level cleaners (require root or elevated privileges).

pub mod journal;
pub mod orphans;
pub mod pacman;
pub mod tmp;
