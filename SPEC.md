# photo_import — Specification

## Overview
CLI tool that organizes photos from a source directory into a date-based folder structure at a destination.

## Usage
```
photo_import <src> <dest> [-m | --move] [--dry-run]
```

## Behavior

### File discovery
- Walk `<src>` recursively
- Process files with extensions: `.arw`, `.jpg`, `.jpeg` (case-insensitive)
- All other files are ignored

### XMP sidecar files
- Darktable sidecar format: `<photo_filename>.xmp` (e.g. `A1_05140.ARW.xmp`)
- When a photo is copied/moved and a corresponding `.xmp` sidecar exists in the source, it is copied/moved to the same destination directory
- If the photo is skipped, the sidecar is not copied/moved
- Same duplicate / conflict handling applies to the sidecar

### Date extraction
- Read EXIF tag `DateTimeOriginal` from each file
- If tag is absent: skip file, log to stderr
- No fallback to mtime or filename

### Destination structure
```
<dest>/YYYY/YYYY-MM-DD/<filename>
```
Example: `dest/2026/2026-05-13/A1_05140.ARW`

### Operation
- Default: **copy** files to destination (source files are kept)
- With `-m` / `--move`: **move** files (delete from source after successful copy)
  - User must confirm by typing `yes` before any files are processed
- With `--dry-run`: print what would happen without copying, moving, or creating anything

### Duplicate / conflict handling
Compute blake3 checksum of source file, then:

| Destination state | Action |
|---|---|
| No file at destination | Copy (or move with `--move`) |
| File exists, same checksum | Skip, log to stderr |
| File exists, different checksum | Rename source file: `A1_05140(1).ARW`, `(2)`, … then copy/move |

Rename counter increments until a free slot is found.

## Output
- Each moved file: print destination path to stdout
- Skipped / renamed / errors: log to stderr

## Progress display
- A progress bar shows `[n/total] current_filename` while processing
- SKIP / RENAME / error messages are printed above the bar so they don't corrupt it


## Implementation
- Language: Rust
- Single binary, single `src/main.rs`
- Dependencies: `clap`, `walkdir`, `kamadak-exif`, `blake3`, `anyhow`, `indicatif`
