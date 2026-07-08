use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

fn binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_phorg"))
}

fn run_move(src: &Path, dest: &Path) -> std::process::Output {
    let mut child = Command::new(binary())
        .args([src, dest, Path::new("--move")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"yes\n").unwrap();
    child.wait_with_output().unwrap()
}

/// Minimal TIFF with a single DateTimeOriginal tag in an Exif sub-IFD.
/// date: "YYYY:MM:DD HH:MM:SS" (exactly 19 chars)
fn make_arw(date: &str) -> Vec<u8> {
    assert_eq!(date.len(), 19);
    let mut date_bytes = date.as_bytes().to_vec();
    date_bytes.push(0); // null-terminate → 20 bytes
    let mut b = Vec::new();
    b.extend_from_slice(b"II");
    b.extend_from_slice(&42u16.to_le_bytes());
    b.extend_from_slice(&8u32.to_le_bytes()); // IFD0 at offset 8
    // IFD0: 1 entry — ExifIFD pointer
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&0x8769u16.to_le_bytes()); // ExifIFD tag
    b.extend_from_slice(&4u16.to_le_bytes());       // type LONG
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&26u32.to_le_bytes()); // ExifIFD at offset 26
    b.extend_from_slice(&0u32.to_le_bytes());  // next IFD = 0
    // ExifIFD at offset 26: 1 entry — DateTimeOriginal
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&0x9003u16.to_le_bytes()); // DateTimeOriginal
    b.extend_from_slice(&2u16.to_le_bytes());       // type ASCII
    b.extend_from_slice(&20u32.to_le_bytes());
    b.extend_from_slice(&44u32.to_le_bytes()); // string at offset 44
    b.extend_from_slice(&0u32.to_le_bytes());  // next IFD = 0
    b.extend_from_slice(&date_bytes);
    b
}

/// Minimal JPEG with an APP1/Exif segment containing the same TIFF structure.
fn make_jpeg(date: &str) -> Vec<u8> {
    let tiff = make_arw(date);
    let app1_len = (2 + 6 + tiff.len()) as u16;
    let mut b = Vec::new();
    b.extend_from_slice(&[0xFF, 0xD8]); // SOI
    b.extend_from_slice(&[0xFF, 0xE1]); // APP1
    b.extend_from_slice(&app1_len.to_be_bytes());
    b.extend_from_slice(b"Exif\0\0");
    b.extend_from_slice(&tiff);
    b.extend_from_slice(&[0xFF, 0xD9]); // EOI
    b
}

/// Minimal TIFF with no DateTimeOriginal (empty IFD0).
fn make_arw_no_exif() -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"II");
    b.extend_from_slice(&42u16.to_le_bytes());
    b.extend_from_slice(&8u32.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes()); // 0 entries
    b.extend_from_slice(&0u32.to_le_bytes()); // next IFD = 0
    b
}

fn write(path: &Path, data: &[u8]) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, data).unwrap();
}

#[test]
fn test_organizes_by_date() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();

    let files = [
        ("session1/A1_05473.ARW", "2026:06:13 10:00:00", "2026/2026-06-13/A1_05473.ARW"),
        ("session2/A1_05479.ARW", "2026:06:14 10:00:00", "2026/2026-06-14/A1_05479.ARW"),
        ("session3/A1_05704.ARW", "2026:06:16 10:00:00", "2026/2026-06-16/A1_05704.ARW"),
        ("session4/A1_06034.ARW", "2026:06:20 10:00:00", "2026/2026-06-20/A1_06034.ARW"),
        ("session5/A1_06156.ARW", "2026:06:25 10:00:00", "2026/2026-06-25/A1_06156.ARW"),
        ("session6/A1_06172.ARW", "2026:06:30 10:00:00", "2026/2026-06-30/A1_06172.ARW"),
        ("session7/A1_06278.ARW", "2026:07:06 10:00:00", "2026/2026-07-06/A1_06278.ARW"),
    ];
    for (rel, date, _) in &files {
        write(&src.join(rel), &make_arw(date));
    }

    let status = Command::new(binary()).args([&src, &dest]).status().unwrap();
    assert!(status.success());

    for (_, _, expected) in &files {
        assert!(dest.join(expected).exists(), "missing: {expected}");
    }
}

#[test]
fn test_dest_created_when_missing() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest-does-not-exist-yet");
    write(&src.join("A1_0001.ARW"), &make_arw("2026:06:13 10:00:00"));

    assert!(!dest.exists());
    let status = Command::new(binary()).args([&src, &dest]).status().unwrap();
    assert!(status.success());
    assert!(dest.join("2026/2026-06-13/A1_0001.ARW").exists());
}

#[test]
fn test_dry_run_makes_no_changes() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("A1_0001.ARW"), &make_arw("2026:06:13 10:00:00"));

    let output = Command::new(binary()).args([&src, &dest, Path::new("--dry-run")]).output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("2026/2026-06-13/A1_0001.ARW"), "expected dest path in stdout: {stdout}");
    assert!(!dest.join("2026/2026-06-13/A1_0001.ARW").exists(), "dry-run must not create dest file");
    assert!(src.join("A1_0001.ARW").exists(), "dry-run must not touch src file");
}

#[test]
fn test_source_dirs_cleaned() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("session/A1_0001.ARW"), &make_arw("2026:06:13 10:00:00"));

    run_move(&src, &dest);

    let subdirs: Vec<_> = walkdir::WalkDir::new(&src)
        .min_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .collect();
    assert!(subdirs.is_empty(), "leftover dirs: {subdirs:?}");
}

#[test]
fn test_duplicate_skip() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    let data = make_arw("2026:06:13 10:00:00");

    write(&src.join("A1_0001.ARW"), &data);
    Command::new(binary()).args([&src, &dest]).status().unwrap();

    write(&src.join("A1_0001.ARW"), &data);
    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("2026/2026-06-13/A1_0001.ARW"), "expected dest path in stdout: {stdout}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("SKIP (duplicate)"));
}

#[test]
fn test_move_duplicate_source_preserved() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    let data = make_arw("2026:06:13 10:00:00");

    write(&src.join("A1_0001.ARW"), &data);
    run_move(&src, &dest);

    write(&src.join("A1_0001.ARW"), &data);
    let output = run_move(&src, &dest);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SKIP (duplicate)"));
    assert!(src.join("A1_0001.ARW").exists(), "duplicate source must not be moved/deleted");
}

#[test]
fn test_move_aborted_without_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("A1_0001.ARW"), &make_arw("2026:06:13 10:00:00"));

    let mut child = Command::new(binary())
        .args([&src, &dest, Path::new("--move")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"no\n").unwrap();
    let output = child.wait_with_output().unwrap();

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("aborted"));
    assert!(src.join("A1_0001.ARW").exists(), "src file must be untouched when move is aborted");
    assert!(!dest.join("2026/2026-06-13/A1_0001.ARW").exists(), "dest must not be created when move is aborted");
}

#[test]
fn test_conflict_rename() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    let conflict_dir = dest.join("2026/2026-06-13");
    fs::create_dir_all(&conflict_dir).unwrap();
    fs::write(conflict_dir.join("A1_0001.ARW"), b"different content").unwrap();

    write(&src.join("A1_0001.ARW"), &make_arw("2026:06:13 10:00:00"));

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    assert!(conflict_dir.join("A1_0001(1).ARW").exists());
    assert_eq!(fs::read(conflict_dir.join("A1_0001.ARW")).unwrap(), b"different content");
}

#[test]
fn test_conflict_rename_in_move_mode() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    let conflict_dir = dest.join("2026/2026-06-13");
    fs::create_dir_all(&conflict_dir).unwrap();
    fs::write(conflict_dir.join("A1_0001.ARW"), b"different content").unwrap();

    let data = make_arw("2026:06:13 10:00:00");
    write(&src.join("A1_0001.ARW"), &data);

    let output = run_move(&src, &dest);
    assert!(output.status.success());
    assert!(conflict_dir.join("A1_0001(1).ARW").exists());
    assert_eq!(fs::read(conflict_dir.join("A1_0001.ARW")).unwrap(), b"different content");
    assert_eq!(fs::read(conflict_dir.join("A1_0001(1).ARW")).unwrap(), data);
    assert!(!src.join("A1_0001.ARW").exists(), "src file must be moved, not copied, into the renamed path");
}

#[test]
fn test_conflict_rename_second_collision() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    let conflict_dir = dest.join("2026/2026-06-13");
    fs::create_dir_all(&conflict_dir).unwrap();
    fs::write(conflict_dir.join("A1_0001.ARW"), b"different content").unwrap();
    fs::write(conflict_dir.join("A1_0001(1).ARW"), b"yet another content").unwrap();

    write(&src.join("A1_0001.ARW"), &make_arw("2026:06:13 10:00:00"));

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    assert!(conflict_dir.join("A1_0001(2).ARW").exists());
    assert_eq!(fs::read(conflict_dir.join("A1_0001.ARW")).unwrap(), b"different content");
    assert_eq!(fs::read(conflict_dir.join("A1_0001(1).ARW")).unwrap(), b"yet another content");
}

#[test]
fn test_conflict_same_length_different_content() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    let conflict_dir = dest.join("2026/2026-06-13");
    fs::create_dir_all(&conflict_dir).unwrap();

    let data = make_arw("2026:06:13 10:00:00");
    let mut altered = data.clone();
    *altered.last_mut().unwrap() ^= 0xFF; // same length, one byte differs

    fs::write(conflict_dir.join("A1_0001.ARW"), &altered).unwrap();
    write(&src.join("A1_0001.ARW"), &data);

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    assert!(
        conflict_dir.join("A1_0001(1).ARW").exists(),
        "same-length differing content must be treated as a conflict, not a duplicate"
    );
    assert_eq!(fs::read(conflict_dir.join("A1_0001.ARW")).unwrap(), altered);
    assert_eq!(fs::read(conflict_dir.join("A1_0001(1).ARW")).unwrap(), data);
}

#[test]
fn test_duplicate_skip_across_chunk_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();

    let mut data = make_arw("2026:06:13 10:00:00");
    data.extend(std::iter::repeat_n(0xCDu8, 200 * 1024)); // exceed the 64KB compare buffer

    write(&src.join("A1_0001.ARW"), &data);
    Command::new(binary()).args([&src, &dest]).status().unwrap();

    write(&src.join("A1_0001.ARW"), &data);
    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("SKIP (duplicate)"));
}

#[test]
fn test_conflict_difference_past_chunk_boundary() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    let conflict_dir = dest.join("2026/2026-06-13");
    fs::create_dir_all(&conflict_dir).unwrap();

    let base = make_arw("2026:06:13 10:00:00");
    let mut dest_data = base.clone();
    dest_data.extend(std::iter::repeat_n(0xCDu8, 200 * 1024));

    let mut src_data = base;
    src_data.extend(std::iter::repeat_n(0xCDu8, 200 * 1024));
    let last = src_data.len() - 1;
    src_data[last] ^= 0xFF; // same length, differs only past the first 64KB chunk

    fs::write(conflict_dir.join("A1_0001.ARW"), &dest_data).unwrap();
    write(&src.join("A1_0001.ARW"), &src_data);

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    assert!(
        conflict_dir.join("A1_0001(1).ARW").exists(),
        "must detect a difference past the first chunk, not report a false duplicate"
    );
}

#[test]
fn test_dest_inside_src_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = src.join("dest");
    fs::create_dir_all(&dest).unwrap();

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("dest must not be inside src"));
}

#[test]
fn test_nonexistent_src_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("does-not-exist");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid src"));
}

#[test]
fn test_jpg_organized() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("IMG_0001.JPG"), &make_jpeg("2023:10:04 13:38:37"));

    let status = Command::new(binary()).args([&src, &dest]).status().unwrap();
    assert!(status.success());
    assert!(dest.join("2023/2023-10-04/IMG_0001.JPG").exists());
}

#[test]
fn test_jpeg_and_mixed_case_extensions_organized() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("photo.jpeg"), &make_jpeg("2023:10:04 13:38:37"));
    write(&src.join("photo2.Arw"), &make_arw("2023:10:04 13:38:37"));

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    assert!(dest.join("2023/2023-10-04/photo.jpeg").exists());
    assert!(dest.join("2023/2023-10-04/photo2.Arw").exists());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Copied 2 files"), "stdout: {stdout}");
    assert!(stdout.contains("1 ARW"), "stdout: {stdout}");
    assert!(stdout.contains("1 JPG"), "stdout: {stdout}");
}

#[test]
fn test_deeply_nested_src() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("aaa/2023-06-24/A1_0001.ARW"), &make_arw("2023:06:24 12:05:39"));

    let status = Command::new(binary()).args([&src, &dest]).status().unwrap();
    assert!(status.success());
    assert!(dest.join("2023/2023-06-24/A1_0001.ARW").exists());
}

#[test]
fn test_unicode_parent_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("aaa/2023-08-ß6/A1_0001.ARW"), &make_arw("2023:08:06 18:14:47"));

    let status = Command::new(binary()).args([&src, &dest]).status().unwrap();
    assert!(status.success());
    assert!(dest.join("2023/2023-08-06/A1_0001.ARW").exists());
}

#[test]
fn test_xmp_moved_with_photo() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("A1_0001.ARW"), &make_arw("2026:06:13 10:00:00"));
    write(&src.join("A1_0001.ARW.xmp"), b"<xmp/>");

    let status = Command::new(binary()).args([&src, &dest]).status().unwrap();
    assert!(status.success());
    assert!(dest.join("2026/2026-06-13/A1_0001.ARW").exists());
    assert!(dest.join("2026/2026-06-13/A1_0001.ARW.xmp").exists());
}

#[test]
fn test_xmp_not_moved_when_photo_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("NO_EXIF.ARW"), &make_arw_no_exif());
    write(&src.join("NO_EXIF.ARW.xmp"), b"<xmp/>");

    let status = Command::new(binary()).args([&src, &dest]).status().unwrap();
    assert!(status.success());
    assert!(!dest.join("2026/2026-06-13/NO_EXIF.ARW.xmp").exists());
    assert!(src.join("NO_EXIF.ARW.xmp").exists());
}

#[test]
fn test_xmp_duplicate_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    let dest_dir = dest.join("2026/2026-06-13");
    fs::create_dir_all(&dest_dir).unwrap();
    write(&dest_dir.join("A1_0001.ARW.xmp"), b"<xmp/>");

    write(&src.join("A1_0001.ARW"), &make_arw("2026:06:13 10:00:00"));
    write(&src.join("A1_0001.ARW.xmp"), b"<xmp/>");

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Copied 1 files"), "stdout: {stdout}");
    assert!(stdout.contains("0 XMP"), "stdout: {stdout}");
    assert!(String::from_utf8_lossy(&output.stderr).contains("SKIP (duplicate)"));
    assert!(dest_dir.join("A1_0001.ARW").exists());
}

#[test]
fn test_xmp_conflict_rename() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    let dest_dir = dest.join("2026/2026-06-13");
    fs::create_dir_all(&dest_dir).unwrap();
    write(&dest_dir.join("A1_0001.ARW.xmp"), b"different xmp content");

    write(&src.join("A1_0001.ARW"), &make_arw("2026:06:13 10:00:00"));
    write(&src.join("A1_0001.ARW.xmp"), b"<xmp-new/>");

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("RENAME conflict"));
    assert_eq!(fs::read(dest_dir.join("A1_0001.ARW.xmp")).unwrap(), b"different xmp content");
    assert_eq!(fs::read(dest_dir.join("A1_0001.ARW(1).xmp")).unwrap(), b"<xmp-new/>");
}

#[test]
fn test_non_target_file_ignored() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("A1_0001.ARW"), &make_arw("2026:06:13 10:00:00"));
    write(&src.join("notes.txt"), b"not a photo");

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Copied 1 files"), "stdout: {stdout}");
    assert!(src.join("notes.txt").exists(), "non-target file must be left in src");
    assert!(
        walkdir::WalkDir::new(&dest).into_iter().filter_map(|e| e.ok()).all(|e| e.file_name() != "notes.txt"),
        "non-target file must not be copied to dest"
    );
}

#[test]
fn test_summary_counts() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("A1_0001.ARW"), &make_arw("2026:06:13 10:00:00"));
    write(&src.join("A1_0001.ARW.xmp"), b"<xmp/>");
    write(&src.join("IMG_0001.JPG"), &make_jpeg("2026:06:13 10:00:00"));

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Copied 3 files"), "stdout: {stdout}");
    assert!(stdout.contains("1 ARW"), "stdout: {stdout}");
    assert!(stdout.contains("1 JPG"), "stdout: {stdout}");
    assert!(stdout.contains("1 XMP"), "stdout: {stdout}");
    assert!(!stdout.contains("duplicate"), "stdout: {stdout}");
}

#[test]
fn test_summary_duplicates() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    let data = make_arw("2026:06:13 10:00:00");
    write(&src.join("A1_0001.ARW"), &data);

    Command::new(binary()).args([&src, &dest]).status().unwrap();

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("1 duplicate(s) skipped"), "stdout: {stdout}");
    assert!(stdout.contains("Copied 0 files"), "stdout: {stdout}");
}

#[test]
fn test_duplicate_log_file_written() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    let data = make_arw("2026:06:13 10:00:00");
    write(&src.join("A1_0001.ARW"), &data);

    Command::new(binary()).args([&src, &dest]).current_dir(tmp.path()).status().unwrap();

    let output = Command::new(binary()).args([&src, &dest]).current_dir(tmp.path()).output().unwrap();
    assert!(output.status.success());

    let log_files: Vec<_> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("duplicates-log-"))
        .collect();
    assert_eq!(log_files.len(), 1, "expected exactly one duplicate log file");

    let content = fs::read_to_string(log_files[0].path()).unwrap();
    assert!(content.contains("A1_0001.ARW"), "log content: {content}");
    assert!(content.contains("2026/2026-06-13/A1_0001.ARW"), "log content: {content}");
    assert!(content.contains(" -> "), "log content: {content}");
}

#[test]
fn test_no_exif_skipped() {
    let tmp = tempfile::tempdir().unwrap();
    let src = tmp.path().join("src");
    let dest = tmp.path().join("dest");
    fs::create_dir_all(&dest).unwrap();
    write(&src.join("NO_EXIF.ARW"), &make_arw_no_exif());

    let output = Command::new(binary()).args([&src, &dest]).output().unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains('/'), "expected no file paths in stdout");
    assert!(String::from_utf8_lossy(&output.stderr).contains("SKIP (no EXIF date)"));
}
