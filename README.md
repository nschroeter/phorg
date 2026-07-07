# phorg

Organizes photos into a `YYYY/YYYY-MM-DD` folder structure based on EXIF date.

## Usage

```
phorg <src> <dest> [-m | --move] [--dry-run]
```

Copies all `.ARW`, `.JPG`, and `.JPEG` files found recursively in `<src>` into `<dest>/YYYY/MM/DD/`.

## Behavior

- Default: files are **copied** (source is kept)
- `-m` / `--move`: files are **moved** (source deleted); requires typing `yes` to confirm
- `--dry-run`: prints what would happen without copying or moving anything
- Files without an EXIF `DateTimeOriginal` tag are skipped (logged to stderr)
- Darktable `.xmp` sidecar files (e.g. `A1_0001.ARW.xmp`) are copied/moved alongside their photo if present
- Duplicate files (same content) at the destination are skipped
- Filename conflicts (same name, different content) are renamed: `A1_0001(1).ARW`, `A1_0001(2).ARW`, etc.
- Empty source directories are removed after a move
- `<dest>` must not be inside `<src>`
- A summary is printed after processing: file counts by type (ARW, JPG, XMP) and number of duplicates skipped

## Supported formats

- Sony ARW (RAW)
- JPEG / JPG

## Building

```
cargo build --release
```

Binary: `target/release/phorg`

### Linux (from macOS)

Requires [Docker](https://www.docker.com/):

```
docker run --rm --platform linux/amd64 \
  -v $(pwd):/app \
  -v ~/.cargo/registry:/usr/local/cargo/registry \
  -w /app rust:latest \
  cargo build --release
```

## Running tests

```
cargo test
```
