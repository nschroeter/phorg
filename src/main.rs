use anyhow::{Context, Result};
use clap::Parser;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
    name = "phorg",
    about = "Organize photos into YYYY/MM/DD folders by EXIF date",
    long_about = "Moves .ARW, .JPG, and .JPEG files from SRC into DEST/YYYY/MM/DD/.\n\
                  Files without an EXIF DateTimeOriginal are skipped.\n\
                  Duplicates (same content) are skipped. Filename conflicts are renamed e.g. A1_0001(1).ARW."
)]
struct Args {
    #[arg(help = "Source directory to import from")]
    src: PathBuf,
    #[arg(help = "Destination directory to organize into")]
    dest: PathBuf,
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
    let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    Ok(blake3::hash(&data))
}

fn dest_path(dest_root: &Path, year: i32, month: u32, day: u32, filename: &str) -> PathBuf {
    dest_root
        .join(format!("{year:04}"))
        .join(format!("{month:02}"))
        .join(format!("{day:02}"))
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

fn move_xmp_sidecar(src_photo: &Path, dest_photo: &Path) -> Result<()> {
    let mut xmp_filename = src_photo.file_name().unwrap().to_os_string();
    xmp_filename.push(".xmp");
    let xmp_src = src_photo.with_file_name(&xmp_filename);
    if !xmp_src.exists() {
        return Ok(());
    }
    let mut xmp_dest = dest_photo.parent().unwrap().join(&xmp_filename);
    if xmp_dest.exists() {
        if checksum(&xmp_src)? == checksum(&xmp_dest)? {
            eprintln!("SKIP (duplicate): {}", xmp_src.display());
            return Ok(());
        }
        xmp_dest = resolve_conflict(&xmp_dest);
        eprintln!("RENAME conflict -> {}", xmp_dest.file_name().unwrap_or_default().to_string_lossy());
    }
    fs::rename(&xmp_src, &xmp_dest)
        .with_context(|| format!("move {} -> {}", xmp_src.display(), xmp_dest.display()))
}

fn is_target(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("arw" | "jpg" | "jpeg")
    )
}

fn main() -> Result<()> {
    let args = Args::parse();

    let src = args.src.canonicalize().context("invalid src")?;
    let dest = fs::canonicalize(&args.dest).unwrap_or_else(|_| args.dest.clone());
    anyhow::ensure!(
        !dest.starts_with(&src),
        "dest must not be inside src"
    );

    for entry in WalkDir::new(&src)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file() && is_target(e.path()))
    {
        let src_path = entry.path();

        let (y, m, d) = match exif_date(src_path) {
            Some(date) => date,
            None => {
                eprintln!("SKIP (no EXIF date): {}", src_path.display());
                continue;
            }
        };

        let filename = src_path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let mut target = dest_path(&dest, y, m, d, filename);

        if target.exists() {
            let src_hash = checksum(src_path)?;
            let dest_hash = checksum(&target)?;
            if src_hash == dest_hash {
                eprintln!("SKIP (duplicate): {}", src_path.display());
                continue;
            }
            target = resolve_conflict(&target);
            eprintln!("RENAME conflict -> {}", target.file_name().unwrap_or_default().to_string_lossy());
        }

        fs::create_dir_all(target.parent().unwrap())
            .with_context(|| format!("create dir {}", target.parent().unwrap().display()))?;

        fs::rename(src_path, &target)
            .with_context(|| format!("move {} -> {}", src_path.display(), target.display()))?;

        println!("{}", target.display());
        move_xmp_sidecar(src_path, &target)?;
    }

    for entry in WalkDir::new(&src).contents_first(true)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
    {
        let _ = fs::remove_dir(entry.path());
    }

    Ok(())
}
