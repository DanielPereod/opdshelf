//! Formats read through external tools, used only when they are installed:
//! PDF through poppler (`pdfinfo`, `pdftoppm`) and RAR/7z/TAR comics through
//! libarchive's `bsdtar`. The Docker image ships both.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use crate::metadata::{self, natural_cmp, BookMeta, CoverSource};
use crate::paths;

const TIMEOUT: Duration = Duration::from_secs(30);
const MAX_OUTPUT: u64 = 64 * 1024 * 1024;

fn tool_exists(name: &str, arg: &str) -> bool {
    Command::new(name)
        .arg(arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

pub fn pdf_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let ok = tool_exists("pdftoppm", "-v") && tool_exists("pdfinfo", "-v");
        if !ok {
            tracing::info!("poppler-utils not found: PDF covers are disabled");
        }
        ok
    })
}

pub fn bsdtar_available() -> bool {
    static AVAILABLE: OnceLock<bool> = OnceLock::new();
    *AVAILABLE.get_or_init(|| {
        let ok = tool_exists("bsdtar", "--version");
        if !ok {
            tracing::info!("bsdtar not found: CBR/CB7/CBT covers and reading are disabled");
        }
        ok
    })
}

/// Runs a command and returns its stdout, killing it after [`TIMEOUT`].
fn run(program: &str, args: &[&std::ffi::OsStr]) -> Result<Vec<u8>, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        stdout.take(MAX_OUTPUT).read_to_end(&mut buf).map(|_| buf)
    });

    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => break status,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{program} timed out"));
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    let output = reader
        .join()
        .map_err(|_| "reader thread panicked")?
        .map_err(|e| e.to_string())?;
    if !status.success() && output.is_empty() {
        return Err(format!("{program} failed with {status}"));
    }
    Ok(output)
}

// ---------------------------------------------------------------------------
// PDF

pub fn read_pdf(path: &Path) -> BookMeta {
    if !pdf_available() {
        return BookMeta::default();
    }
    let info = run("pdfinfo", &["-enc".as_ref(), "UTF-8".as_ref(), path.as_os_str()]).unwrap_or_default();
    let info = String::from_utf8_lossy(&info);
    let field = |name: &str| {
        info.lines()
            .find_map(|l| l.strip_prefix(name)?.strip_prefix(':'))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    BookMeta {
        title: field("Title").filter(|t| !t.eq_ignore_ascii_case("untitled")),
        authors: field("Author")
            .map(|a| {
                a.split(';')
                    .map(|a| a.trim().to_string())
                    .filter(|a| !a.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        description: field("Subject"),
        subjects: field("Keywords")
            .map(|k| {
                k.split([',', ';'])
                    .map(|k| k.trim().to_string())
                    .filter(|k| !k.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        cover: Some(CoverSource::Pdf),
        ..Default::default()
    }
}

/// Renders the first page as a JPEG.
pub fn pdf_cover(path: &Path) -> Result<Vec<u8>, String> {
    let args: [&std::ffi::OsStr; 11] = [
        "-jpeg".as_ref(),
        "-jpegopt".as_ref(),
        "quality=85".as_ref(),
        "-f".as_ref(),
        "1".as_ref(),
        "-l".as_ref(),
        "1".as_ref(),
        "-scale-to".as_ref(),
        "1200".as_ref(),
        "-singlefile".as_ref(),
        path.as_os_str(),
    ];
    let jpeg = run("pdftoppm", &args)?;
    if jpeg.starts_with(&[0xff, 0xd8]) {
        Ok(jpeg)
    } else {
        Err("pdftoppm produced no image".into())
    }
}

// ---------------------------------------------------------------------------
// RAR / 7z / TAR

pub fn is_archive_comic(rel: &str) -> bool {
    matches!(paths::extension(rel).as_str(), "cbr" | "cb7" | "cbt" | "rar")
}

pub fn archive_entries(path: &Path) -> Result<Vec<String>, String> {
    let list = run("bsdtar", &["-tf".as_ref(), path.as_os_str()])?;
    Ok(String::from_utf8_lossy(&list)
        .lines()
        .filter(|l| !l.is_empty() && !l.ends_with('/'))
        .map(str::to_string)
        .collect())
}

/// bsdtar treats member names as glob patterns, so escape the special characters.
fn escape_pattern(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '\\' | '*' | '?' | '[' | ']') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

pub fn archive_entry(path: &Path, entry: &str) -> Result<Vec<u8>, String> {
    let pattern = escape_pattern(entry);
    let data = run(
        "bsdtar",
        &["-xOf".as_ref(), path.as_os_str(), "--".as_ref(), pattern.as_ref()],
    )?;
    if data.is_empty() {
        return Err(format!("{entry} not found"));
    }
    Ok(data)
}

pub fn read_archive_comic(path: &Path) -> BookMeta {
    if !bsdtar_available() {
        return BookMeta::default();
    }
    let Ok(entries) = archive_entries(path) else {
        return BookMeta::default();
    };
    let mut meta = entries
        .iter()
        .find(|e| paths::file_name(e).eq_ignore_ascii_case("ComicInfo.xml"))
        .and_then(|e| archive_entry(path, e).ok())
        .and_then(|xml| metadata::parse_comic_info(&String::from_utf8_lossy(&xml)).ok())
        .unwrap_or_default();
    meta.cover = pick_cover(entries.iter().map(String::as_str)).map(|(name, mime)| CoverSource::Archive { name, mime });
    meta
}

/// Image entries of a comic in reading order.
pub fn sorted_images<'a>(names: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut images: Vec<&str> = names
        .filter(|n| {
            metadata::image_mime(n).is_some() && !n.contains("__MACOSX") && !paths::file_name(n).starts_with('.')
        })
        .collect();
    images.sort_by(|a, b| natural_cmp(a, b));
    images.into_iter().map(str::to_string).collect()
}

/// Picks `cover.*` if present, otherwise the first page.
pub fn pick_cover<'a>(names: impl Iterator<Item = &'a str>) -> Option<(String, String)> {
    let images = sorted_images(names);
    let name = images
        .iter()
        .find(|n| paths::stem(n).eq_ignore_ascii_case("cover"))
        .or_else(|| images.first())?
        .clone();
    let mime = metadata::image_mime(&name)?.to_string();
    Some((name, mime))
}

/// Pages of a comic book (CBZ/ZIP or, with bsdtar, CBR/CB7/CBT).
pub fn comic_pages(path: &Path) -> Result<Vec<String>, String> {
    let rel = path.to_string_lossy();
    if is_archive_comic(&rel) {
        if !bsdtar_available() {
            return Err("bsdtar is not installed".into());
        }
        let entries = archive_entries(path)?;
        return Ok(sorted_images(entries.iter().map(String::as_str)));
    }
    let zip = metadata::open_zip(path)?;
    Ok(sorted_images(zip.file_names()))
}

pub fn comic_page(path: &Path, name: &str) -> Result<Vec<u8>, String> {
    if is_archive_comic(&path.to_string_lossy()) {
        archive_entry(path, name)
    } else {
        metadata::open_zip(path).and_then(|mut zip| metadata::read_entry(&mut zip, name, MAX_OUTPUT))
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn tiny_pdf(path: &Path, title: &str) {
        let pdf = format!(
            "%PDF-1.1\n1 0 obj<</Type/Catalog/Pages 2 0 R>>endobj 2 0 obj<</Type/Pages/Kids[3 0 R]/Count 1>>endobj \
             3 0 obj<</Type/Page/Parent 2 0 R/MediaBox[0 0 200 300]>>endobj \
             4 0 obj<</Title({title})/Author(Ana; Luis)>>endobj\ntrailer<</Root 1 0 R/Info 4 0 R>>\n"
        );
        std::fs::write(path, pdf).unwrap();
    }

    #[test]
    fn pdf_metadata_and_cover() {
        if !pdf_available() {
            eprintln!("poppler not installed, skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.pdf");
        tiny_pdf(&path, "Mi PDF");
        let meta = read_pdf(&path);
        assert_eq!(meta.title.as_deref(), Some("Mi PDF"));
        assert_eq!(meta.authors, vec!["Ana", "Luis"]);
        assert!(pdf_cover(&path).unwrap().starts_with(&[0xff, 0xd8]));
    }

    #[test]
    fn archive_comic_with_bsdtar() {
        if !bsdtar_available() {
            eprintln!("bsdtar not installed, skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("p10.jpg"), b"ten").unwrap();
        std::fs::write(src.join("p2 [x].jpg"), b"two").unwrap();
        std::fs::write(
            src.join("ComicInfo.xml"),
            b"<ComicInfo><Title>Siete</Title></ComicInfo>",
        )
        .unwrap();
        let path = dir.path().join("c.cb7");
        let status = Command::new("bsdtar")
            .args(["--format", "7zip", "-cf"])
            .arg(&path)
            .arg("-C")
            .arg(&src)
            .args(["p10.jpg", "p2 [x].jpg", "ComicInfo.xml"])
            .status()
            .unwrap();
        assert!(status.success());

        let meta = read_archive_comic(&path);
        assert_eq!(meta.title.as_deref(), Some("Siete"));
        let Some(CoverSource::Archive { name, .. }) = meta.cover else {
            panic!("no cover")
        };
        assert_eq!(name, "p2 [x].jpg");
        assert_eq!(comic_pages(&path).unwrap(), vec!["p2 [x].jpg", "p10.jpg"]);
        assert_eq!(comic_page(&path, "p2 [x].jpg").unwrap(), b"two");
    }
}
