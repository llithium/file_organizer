use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use clap::Parser;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "tiff", "webp"];
const VIDEO_EXTS: &[&str] = &["mp4", "mov", "avi", "mkv", "flv", "wmv", "webm", "mpeg", "m4v"];
const AUDIO_EXTS: &[&str] = &["mp3", "flac", "aac", "wav", "ogg", "m4a", "wma", "opus"];
const DOC_EXTS: &[&str] = &[
    "pdf", "doc", "docx", "txt", "rtf", "odt", "md", "xls", "xlsx", "ppt", "pptx", "csv",
];

/// Category folders, in the order they are checked.
const CATEGORIES: &[(&str, &[&str])] = &[
    ("Images", IMAGE_EXTS),
    ("Videos", VIDEO_EXTS),
    ("Audio", AUDIO_EXTS),
    ("Documents", DOC_EXTS),
];

/// Organizes files into category folders (Images, Videos, Audio, Documents).
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Target directory to organize (defaults to current directory)
    target: Option<String>,

    /// Recurse into subdirectories (category folders are never descended into)
    #[arg(short, long)]
    recursive: bool,

    /// Preview what would happen without moving any files
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Remove subdirectories left empty after organizing (requires --recursive)
    #[arg(short = 'd', long, requires = "recursive")]
    delete_empty_dirs: bool,

    /// Print each file operation as it is performed
    #[arg(short, long)]
    verbose: bool,
}

fn move_file(src: &Path, dst: &Path) -> io::Result<()> {
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(rename_err) => {
            // Cross-device move: fall back to copy + delete
            if rename_err.raw_os_error() == Some(libc::EXDEV) {
                fs::copy(src, dst)?;
                fs::remove_file(src)?;
                Ok(())
            } else {
                Err(rename_err)
            }
        }
    }
}

fn unique_path(dir: &Path, stem: &str, ext: &str) -> PathBuf {
    // Try the original name first before appending a counter
    let original = if ext.is_empty() {
        dir.join(stem)
    } else {
        dir.join(format!("{}.{}", stem, ext))
    };
    if !original.exists() {
        return original;
    }

    let mut counter = 1u32;
    loop {
        let name = if ext.is_empty() {
            format!("{}_{}", stem, counter)
        } else {
            format!("{}_{}.{}", stem, counter, ext)
        };
        let candidate = dir.join(&name);
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

/// Returns the category folder name for a given lowercase extension, if any.
fn category_for(ext: &str) -> Option<&'static str> {
    CATEGORIES
        .iter()
        .find(|(_, exts)| exts.contains(&ext))
        .map(|(name, _)| *name)
}

struct Stats {
    moved: u32,
    skipped: u32,
    removed_dirs: u32,
}

fn organize_files(args: &Args, root: &Path) -> io::Result<()> {
    // Ensure every category folder is usable (exists as a directory) up front.
    for (name, _) in CATEGORIES {
        let dir = root.join(name);
        if dir.exists() && !dir.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} exists but is not a directory", dir.display()),
            ));
        }
    }

    let category_names: Vec<PathBuf> = CATEGORIES.iter().map(|(n, _)| root.join(n)).collect();

    let mut stats = Stats {
        moved: 0,
        skipped: 0,
        removed_dirs: 0,
    };
    organize_dir(args, root, root, &category_names, &mut stats)?;

    let label = if args.dry_run { "would move" } else { "moved" };
    print!("\nDone: {} {}, {} skipped", stats.moved, label, stats.skipped);
    if args.delete_empty_dirs {
        let dir_label = if args.dry_run {
            "would remove"
        } else {
            "removed"
        };
        print!(", {} {} empty dir(s)", stats.removed_dirs, dir_label);
    }
    println!(".");
    Ok(())
}

/// Organizes a single directory, recursing into subdirectories when requested.
///
/// Returns `true` if the directory is (or, in dry-run mode, would be) empty
/// once organizing is complete, so the caller can prune it when
/// `--delete-empty-dirs` is set.
fn organize_dir(
    args: &Args,
    root: &Path,
    dir_path: &Path,
    category_dirs: &[PathBuf],
    stats: &mut Stats,
) -> io::Result<bool> {
    let mut subdirs: Vec<PathBuf> = Vec::new();
    // Count entries that will remain in this directory after organizing.
    let mut remaining = 0u32;

    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            // Never descend into the category folders themselves.
            if category_dirs.iter().any(|c| c == &path) {
                remaining += 1;
            } else {
                subdirs.push(path);
            }
            continue;
        }

        if !path.is_file() {
            remaining += 1;
            continue;
        }

        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_lowercase(),
            None => {
                remaining += 1;
                continue;
            }
        };

        let category = match category_for(&ext) {
            Some(c) => c,
            None => {
                remaining += 1;
                continue;
            }
        };

        let target_dir = root.join(category);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
        let target_path = unique_path(&target_dir, stem, &ext);

        if args.dry_run {
            stats.moved += 1;
            println!("[dry-run] {} -> {}", path.display(), target_path.display());
            continue;
        }

        // Create the category folder lazily, only when it's actually needed.
        if let Err(e) = fs::create_dir_all(&target_dir) {
            eprintln!("Skipped {}: {}", path.display(), e);
            stats.skipped += 1;
            remaining += 1;
            continue;
        }

        match move_file(&path, &target_path) {
            Ok(()) => {
                stats.moved += 1;
                if args.verbose {
                    println!("{} -> {}", path.display(), target_path.display());
                }
            }
            Err(e) => {
                eprintln!("Skipped {}: {}", path.display(), e);
                stats.skipped += 1;
                remaining += 1;
            }
        }
    }

    if args.recursive {
        for subdir in subdirs {
            let child_empty = organize_dir(args, root, &subdir, category_dirs, stats)?;

            if args.delete_empty_dirs && child_empty {
                if args.dry_run {
                    stats.removed_dirs += 1;
                    println!("[dry-run] remove empty dir {}", subdir.display());
                } else if let Err(e) = fs::remove_dir(&subdir) {
                    eprintln!("Could not remove {}: {}", subdir.display(), e);
                    remaining += 1;
                } else {
                    stats.removed_dirs += 1;
                    if args.verbose {
                        println!("removed empty dir {}", subdir.display());
                    }
                }
            } else {
                // Subdir left in place (kept, or not eligible for pruning).
                remaining += 1;
            }
        }
    } else {
        // Without recursion, subdirectories are left untouched.
        remaining += subdirs.len() as u32;
    }

    Ok(remaining == 0)
}

fn main() {
    let args = Args::parse();
    let target = args.target.clone().unwrap_or_else(|| ".".to_string());
    if let Err(e) = organize_files(&args, Path::new(&target)) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}
