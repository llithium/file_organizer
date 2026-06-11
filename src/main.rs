use std::env;
use std::fs;
use std::io;
use std::path::Path;

const IMAGE_EXTS: &[&str] = &["jpg", "jpeg", "png", "gif", "bmp", "tiff", "webp"];
const VIDEO_EXTS: &[&str] = &["mp4", "mov", "avi", "mkv", "flv", "wmv", "webm", "mpeg", "m4v"];

fn move_file(src: &Path, dst: &Path) -> io::Result<()> {
    // Fallback to copy+delete for cross-device moves
    if let Err(_) = fs::rename(src, dst) {
        fs::copy(src, dst)?;
        fs::remove_file(src)?;
    }
    Ok(())
}

fn unique_path(dir: &Path, stem: &str, ext: &str) -> std::path::PathBuf {
    let mut counter = 1;
    loop {
        let name = format!("{}_{}.{}", stem, counter, ext);
        let candidate = dir.join(&name);
        if !candidate.exists() {
            return candidate;
        }
        counter += 1;
    }
}

fn organize_files(dir_path: &Path) -> io::Result<()> {
    let img_dir = dir_path.join("Images");
    let vid_dir = dir_path.join("Videos");

    fs::create_dir_all(&img_dir)?;
    fs::create_dir_all(&vid_dir)?;

    for entry in fs::read_dir(dir_path)? {
        let entry = entry?;
        let path = entry.path();

        // Skip subdirectories (including our own Images/ and Videos/)
        if !path.is_file() {
            continue;
        }

        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e.to_lowercase(),
            None => continue,
        };

        let target_dir = if IMAGE_EXTS.contains(&ext.as_str()) {
            &img_dir
        } else if VIDEO_EXTS.contains(&ext.as_str()) {
            &vid_dir
        } else {
            continue;
        };

        let file_name = match path.file_name() {
            Some(n) => n,
            None => continue,
        };

        let mut target_path = target_dir.join(file_name);

        if target_path.exists() {
            let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
            target_path = unique_path(target_dir, stem, &ext);
        }

        if let Err(e) = move_file(&path, &target_path) {
            eprintln!("Skipped {}: {}", path.display(), e);
        }
    }

    Ok(())
}

fn main() {
    let dir = env::args().nth(1).unwrap_or_else(|| ".".to_string());
    if let Err(e) = organize_files(Path::new(&dir)) {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}