# photo_import — Specification

## Overview
CLI tool that organizes photos from a source directory into a date-based folder structure at a destination.

## Usage
```
photo_import <src> <dest>
```

## Behavior

### File discovery
- Walk `<src>` recursively
- Process files with extensions: `.arw`, `.jpg`, `.jpeg` (case-insensitive)
- All other files are ignored

### Date extraction
- Read EXIF tag `DateTimeOriginal` from each file
- If tag is absent: skip file, log to stderr
- No fallback to mtime or filename

### Destination structure
```
<dest>/YYYY/MM/DD/<filename>
```
Example: `dest/2026/05/13/A1_05140.ARW`

### Operation
- **Move** files (delete from source after successful copy)

### Duplicate / conflict handling
Compute blake3 checksum of source file, then:

| Destination state | Action |
|---|---|
| No file at destination | Move |
| File exists, same checksum | Skip, log to stderr |
| File exists, different checksum | Rename source file: `A1_05140(1).ARW`, `(2)`, … then move |

Rename counter increments until a free slot is found.

## Output
- Each moved file: print destination path to stdout
- Skipped / renamed / errors: log to stderr

## Implementation
- Language: Rust
- Single binary, single `src/main.rs`
- Dependencies: `clap`, `walkdir`, `kamadak-exif`, `blake3`, `anyhow`
