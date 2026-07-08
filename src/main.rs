use anyhow::{Context, Result};
use chrono::Local;
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::time::Duration;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
    name = "phorg",
    about = "Organize photos into YYYY/MM/DD folders by EXIF date",
    long_about = "Copies .ARW, .JPG, and .JPEG files from SRC into DEST/YYYY/MM/DD/ based on EXIF date.\n\
                  Darktable .xmp sidecar files are copied/moved alongside their photo.\n\
                  Files without an EXIF DateTimeOriginal are skipped.\n\
                  Duplicates (same content) are skipped. Filename conflicts are renamed e.g. A1_0001(1).ARW.\n\
                  Use --move to move instead of copy (requires confirmation)."
)]
struct Args {
    #[arg(help = "Source directory to import from")]
    src: PathBuf,
    #[arg(help = "Destination directory to organize into")]
    dest: PathBuf,
    #[arg(short = 'm', long = "move", help = "Move files instead of copying; requires confirmation")]
    move_files: bool,
    #[arg(long, help = "Print what would happen without copying or moving anything")]
    dry_run: bool,
}

fn exif_date(path: &Path) -> Option<(i32, u32, u32)> {
    let file = fs::File::open(path).ok()?;
    let reader = exif::Reader::new();
    let exif = reader.read_from_container(&mut BufReader::new(file)).ok()?;
    let field = exif.get_field(exif::Tag::DateTimeOriginal, exif::In::PRIMARY)?;
    // Value is Ascii "YYYY:MM:DD HH:MM:SS"
    let exif::Value::Ascii(ref vec) = field.value else { return None };
    let raw = vec.first()?;
    let s = std::str::from_utf8(raw).ok()?;
    let date_part = s.splitn(2, ' ').next()?;
    let parts: Vec<&str> = date_part.split(':').collect();
    if parts.len() != 3 {
        return None;
    }
    Some((parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?))
}

fn checksum(path: &Path) -> Result<blake3::Hash> {
    let file = fs::File::open(path).with_context(|| format!("read {}", path.display()))?;
    let mut hasher = blake3::Hasher::new();
    hasher.update_reader(BufReader::new(file)).with_context(|| format!("read {}", path.display()))?;
    Ok(hasher.finalize())
}

fn same_content(a: &Path, b: &Path) -> Result<bool> {
    let len_a = fs::metadata(a).with_context(|| format!("stat {}", a.display()))?.len();
    let len_b = fs::metadata(b).with_context(|| format!("stat {}", b.display()))?.len();
    if len_a != len_b {
        return Ok(false);
    }
    Ok(checksum(a)? == checksum(b)?)
}

fn dest_path(dest_root: &Path, year: i32, month: u32, day: u32, filename: &str) -> PathBuf {
    dest_root
        .join(format!("{year:04}"))
        .join(format!("{year:04}-{month:02}-{day:02}"))
        .join(filename)
}

fn resolve_conflict(base: &Path) -> PathBuf {
    let stem = base.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
    let ext = base.extension().and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let dir = base.parent().unwrap_or(Path::new("."));
    let mut i = 1u32;
    loop {
        let candidate = dir.join(format!("{stem}({i}){ext}"));
        if !candidate.exists() {
            return candidate;
        }
        i += 1;
    }
}

fn transfer_xmp_sidecar(src_photo: &Path, dest_photo: &Path, move_files: bool, dry_run: bool, pb: &ProgressBar) -> Result<bool> {
    let mut xmp_filename = src_photo.file_name().unwrap().to_os_string();
    xmp_filename.push(".xmp");
    let xmp_src = src_photo.with_file_name(&xmp_filename);
    if !xmp_src.exists() {
        return Ok(false);
    }
    let mut xmp_dest = dest_photo.parent().unwrap().join(&xmp_filename);
    if xmp_dest.exists() {
        if same_content(&xmp_src, &xmp_dest)? {
            pb.suspend(|| eprintln!("SKIP (duplicate): {}", xmp_src.display()));
            return Ok(false);
        }
        xmp_dest = resolve_conflict(&xmp_dest);
        pb.suspend(|| eprintln!("RENAME conflict -> {}", xmp_dest.file_name().unwrap_or_default().to_string_lossy()));
    }
    if !dry_run {
        if move_files {
            fs::rename(&xmp_src, &xmp_dest)
                .with_context(|| format!("move {} -> {}", xmp_src.display(), xmp_dest.display()))?;
        } else {
            fs::copy(&xmp_src, &xmp_dest)
                .with_context(|| format!("copy {} -> {}", xmp_src.display(), xmp_dest.display()))?;
        }
    }
    Ok(true)
}

#[derive(Default)]
struct Stats {
    arw: u32,
    jpg: u32,
    xmp: u32,
    duplicate_paths: Vec<String>,
}

fn is_target(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("arw" | "jpg" | "jpeg")
    )
}

fn main() -> Result<()> {
    let args = Args::parse();

    let dry_run = args.dry_run;
    let move_files = args.move_files;
    let src = args.src.canonicalize().context("invalid src")?;
    let dest = fs::canonicalize(&args.dest).unwrap_or_else(|_| args.dest.clone());
    anyhow::ensure!(
        !dest.starts_with(&src),
        "dest must not be inside src"
    );

    if move_files && !dry_run {
        eprint!("This will move files from {} to {}. Type 'yes' to confirm: ", src.display(), dest.display());
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).context("failed to read confirmation")?;
        anyhow::ensure!(input.trim() == "yes", "aborted");
    }

    let entries: Vec<_> = WalkDir::new(&src)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_target(e.path()))
        .collect();

    let pb = ProgressBar::new(entries.len() as u64);
    pb.set_prefix(if move_files { "Moving" } else { "Copying" });
    pb.set_style(
        ProgressStyle::with_template("{spinner:.dim} {prefix} [{bar:40}] {pos}/{len} {msg}")?
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏ "),
    );
    pb.enable_steady_tick(Duration::from_millis(100));

    let mut stats = Stats::default();

    for entry in &entries {
        let src_path = entry.path();
        pb.set_message(src_path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string());

        let (y, m, d) = match exif_date(src_path) {
            Some(date) => date,
            None => {
                pb.suspend(|| eprintln!("SKIP (no EXIF date): {}", src_path.display()));
                pb.inc(1);
                continue;
            }
        };

        let filename = src_path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let mut target = dest_path(&dest, y, m, d, filename);

        if target.exists() {
            if same_content(src_path, &target)? {
                let src_str = src_path.display().to_string();
                let dest_str = target.display().to_string();
                pb.suspend(|| eprintln!("SKIP (duplicate): {src_str}"));
                pb.suspend(|| println!("{dest_str}"));
                stats.duplicate_paths.push(format!("{src_str} -> {dest_str}"));
                pb.inc(1);
                continue;
            }
            target = resolve_conflict(&target);
            pb.suspend(|| eprintln!("RENAME conflict -> {}", target.file_name().unwrap_or_default().to_string_lossy()));
        }

        if !dry_run {
            fs::create_dir_all(target.parent().unwrap())
                .with_context(|| format!("create dir {}", target.parent().unwrap().display()))?;
            if move_files {
                fs::rename(src_path, &target)
                    .with_context(|| format!("move {} -> {}", src_path.display(), target.display()))?;
            } else {
                fs::copy(src_path, &target)
                    .with_context(|| format!("copy {} -> {}", src_path.display(), target.display()))?;
            }
        }

        match src_path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref() {
            Some("arw") => stats.arw += 1,
            Some("jpg" | "jpeg") => stats.jpg += 1,
            _ => {}
        }

        pb.suspend(|| println!("{}", target.display()));
        if transfer_xmp_sidecar(src_path, &target, move_files, dry_run, &pb)? {
            stats.xmp += 1;
        }
        pb.inc(1);
    }
    pb.finish_and_clear();

    let verb = if move_files { "Moved" } else { "Copied" };
    let total = stats.arw + stats.jpg + stats.xmp;
    println!("{verb} {total} files — {} ARW, {} JPG, {} XMP", stats.arw, stats.jpg, stats.xmp);
    if !stats.duplicate_paths.is_empty() {
        println!("{} duplicate(s) skipped", stats.duplicate_paths.len());
        let log_name = format!("duplicates-log-{}.log", Local::now().format("%Y-%m-%d_%H-%M-%S"));
        fs::write(&log_name, stats.duplicate_paths.join("\n") + "\n")
            .with_context(|| format!("write {log_name}"))?;
    }

    if move_files && !dry_run {
        for entry in WalkDir::new(&src).contents_first(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_dir())
        {
            let _ = fs::remove_dir(entry.path());
        }
    }

    Ok(())
}
