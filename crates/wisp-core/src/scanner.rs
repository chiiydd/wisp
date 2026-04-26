//! Async directory scanner (L2).
//!
//! Uses `jwalk` (rayon-based parallel walker) inside
//! `tokio::task::spawn_blocking` so L4/L5 only ever see a single async API.

use std::collections::HashMap;
use std::path::PathBuf;

use camino::Utf8PathBuf;
use tokio::task;
use tracing::instrument;

use crate::errors::{CoreError, CoreResult};
use crate::types::{ScanKey, ScanNode, ScanTree};

// ─── Options ─────────────────────────────────────────────────────────────────

/// Options controlling how a directory is scanned.
#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    /// Maximum directory depth.  `None` means unlimited.
    pub max_depth: Option<usize>,
    /// Skip entries whose size is below this threshold.
    pub min_size: Option<u64>,
    /// Follow symbolic links.
    pub follow_symlinks: bool,
}

// ─── Async entry point ────────────────────────────────────────────────────────

/// Scan `root` asynchronously by offloading CPU work to a blocking thread.
#[instrument(name = "wisp.scan", skip(opts), fields(root = %root))]
pub async fn scan_directory(root: Utf8PathBuf, opts: ScanOptions) -> CoreResult<ScanTree> {
    task::spawn_blocking(move || scan_blocking(root, opts))
        .await
        .map_err(|e| CoreError::Io(std::io::Error::other(e.to_string())))?
}

// ─── Blocking implementation ──────────────────────────────────────────────────

fn scan_blocking(root: Utf8PathBuf, opts: ScanOptions) -> CoreResult<ScanTree> {
    use jwalk::{Parallelism, WalkDir};

    // Dedicated rayon pool sized to CPU count — avoids contention with tokio's
    // blocking threadpool and ensures full parallelism even when called from
    // inside spawn_blocking.
    let n_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);

    let mut tree = ScanTree::default();
    let mut path_to_key: HashMap<PathBuf, ScanKey> = HashMap::new();

    for entry_result in WalkDir::new(root.as_std_path())
        .follow_links(opts.follow_symlinks)
        .skip_hidden(false)
        .sort(true)
        .parallelism(Parallelism::RayonNewPool(n_threads))
    {
        let entry = match entry_result {
            Ok(e) => e,
            Err(_) => continue,
        };

        // Respect max depth
        if let Some(max) = opts.max_depth
            && entry.depth() > max
        {
            continue;
        }

        let path = entry.path();
        let (file_size, is_dir) = match entry.metadata() {
            Ok(m) => (if m.is_file() { m.len() } else { 0 }, m.is_dir()),
            Err(_) => (0, false),
        };

        // Skip non-UTF-8 paths gracefully
        let utf8_path = match Utf8PathBuf::from_path_buf(path.clone()) {
            Ok(p) => p,
            Err(_) => continue,
        };

        let key = tree.nodes.insert(ScanNode {
            path: utf8_path,
            size: file_size,
            children: Vec::new(),
            is_dir,
        });

        // First entry is the root
        if tree.root.is_none() {
            tree.root = Some(key);
        }

        // Link to parent (parent was always visited before children with sort=true)
        if let Some(parent) = path.parent()
            && let Some(&parent_key) = path_to_key.get(parent)
        {
            tree.nodes[parent_key].children.push(key);
        }

        path_to_key.insert(path, key);
    }

    // Accumulate directory sizes bottom-up
    if let Some(root_key) = tree.root {
        accumulate_sizes(&mut tree, root_key);
    }

    Ok(tree)
}

fn accumulate_sizes(tree: &mut ScanTree, key: ScanKey) -> u64 {
    let children: Vec<ScanKey> = tree.nodes[key].children.clone();
    let own_size = tree.nodes[key].size;
    let child_total: u64 = children.iter().map(|&ck| accumulate_sizes(tree, ck)).sum();
    let total = own_size + child_total;
    tree.nodes[key].size = total;
    total
}

// ─── Display helpers ──────────────────────────────────────────────────────────

/// Return entries sorted by size, largest first.
pub fn top_entries(tree: &ScanTree, n: usize) -> Vec<(&Utf8PathBuf, u64, bool)> {
    let mut entries: Vec<_> = tree
        .nodes
        .values()
        .map(|n| (&n.path, n.size, n.is_dir))
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.1));
    entries.truncate(n);
    entries
}

/// Render the scan tree as an indented text tree.
pub fn format_tree(tree: &ScanTree, max_depth: usize, max_children: usize) -> String {
    let mut out = String::new();
    if let Some(root_key) = tree.root {
        fmt_node(
            tree,
            root_key,
            0,
            max_depth,
            max_children,
            &mut out,
            true,
            "",
        );
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn fmt_node(
    tree: &ScanTree,
    key: ScanKey,
    depth: usize,
    max_depth: usize,
    max_children: usize,
    out: &mut String,
    is_last: bool,
    prefix: &str,
) {
    let node = &tree.nodes[key];
    let size_str = humansize::format_size(node.size, humansize::DECIMAL);

    let name: &str = if depth == 0 {
        node.path.as_str()
    } else {
        node.path.file_name().unwrap_or(node.path.as_str())
    };

    let connector = if depth == 0 {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };
    let dir_mark = if node.is_dir && depth > 0 { "/" } else { "" };

    out.push_str(&format!(
        "{prefix}{connector}{size_str:>10}  {name}{dir_mark}\n"
    ));

    if depth >= max_depth {
        return;
    }

    let mut children: Vec<ScanKey> = node.children.clone();
    children.sort_by_key(|c| std::cmp::Reverse(tree.nodes[*c].size));
    let truncated = children.len() > max_children;
    children.truncate(max_children);

    let child_prefix = if depth == 0 {
        prefix.to_owned()
    } else {
        format!("{prefix}{}   ", if is_last { ' ' } else { '│' })
    };

    let n = children.len();
    for (i, &ck) in children.iter().enumerate() {
        let last = i == n - 1 && !truncated;
        fmt_node(
            tree,
            ck,
            depth + 1,
            max_depth,
            max_children,
            out,
            last,
            &child_prefix,
        );
    }

    if truncated {
        let remaining = tree.nodes[key].children.len() - max_children;
        out.push_str(&format!("{child_prefix}└── … ({remaining} more)\n"));
    }
}

/// Render a flat sorted list of top entries.
pub fn format_flat(tree: &ScanTree, top: usize) -> String {
    let mut out = String::new();
    for (path, size, is_dir) in top_entries(tree, top) {
        let size_str = humansize::format_size(size, humansize::DECIMAL);
        let mark = if is_dir { "/" } else { "" };
        out.push_str(&format!("{size_str:>10}  {path}{mark}\n"));
    }
    out
}
