use std::fmt::Write as FmtWrite;
use std::io::Write;
use std::path::PathBuf;

use crate::scanner::Entry;

pub fn generate_report(root: &Entry, min_bytes: u64) -> String {
    let mut out = String::new();
    let total = humansize::format_size(root.size, humansize::BINARY);

    let _ = writeln!(out, "DUSK Report: {} ({})", root.path.display(), total);
    let _ = writeln!(out, "{}", "=".repeat(72));
    let _ = writeln!(out);

    write_tree(&mut out, root, min_bytes, 0);

    let _ = writeln!(out);
    let _ = writeln!(out, "Minimum display threshold: {}", humansize::format_size(min_bytes, humansize::BINARY));

    out
}

fn write_tree(out: &mut String, entry: &Entry, min_bytes: u64, depth: usize) {
    let mut children: Vec<&Entry> = entry.children.iter().filter(|c| c.size >= min_bytes).collect();
    children.sort_by(|a, b| b.size.cmp(&a.size));

    for child in &children {
        let size_str = humansize::format_size(child.size, humansize::BINARY);
        let indent = "  ".repeat(depth);
        let icon = if child.is_dir { "/" } else { "" };

        let _ = writeln!(out, "{}{:>10}  {}{}", indent, size_str, child.name, icon);

        if child.is_dir {
            write_tree(out, child, min_bytes, depth + 1);
        }
    }
}

pub fn export_report(root: &Entry, min_bytes: u64) -> Result<PathBuf, std::io::Error> {
    let report = generate_report(root, min_bytes);

    let dir_name = root
        .path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "root".to_string())
        .replace(['\\', '/', ':', '*', '?', '"', '<', '>', '|'], "_");

    let filename = format!("dusk-report-{}.txt", dir_name);
    let path = std::env::current_dir()?.join(&filename);

    let mut file = std::fs::File::create(&path)?;
    file.write_all(report.as_bytes())?;

    Ok(path)
}

