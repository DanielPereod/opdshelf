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
    run_with(program, args, &[])
}

/// Like [`run`], with environment variables set for the command.
fn run_with(program: &str, args: &[&std::ffi::OsStr], env: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    let mut child = Command::new(program)
        .args(args)
        .envs(env.iter().copied())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    // One byte more than allowed, to know the output was cut
    let too_big = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = too_big.clone();
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let result = stdout.take(MAX_OUTPUT + 1).read_to_end(&mut buf);
        flag.store(buf.len() as u64 > MAX_OUTPUT, std::sync::atomic::Ordering::SeqCst);
        result.map(|_| buf)
    });
    let errors = std::thread::spawn(move || {
        let mut stderr = stderr;
        let mut kept = Vec::new();
        let _ = (&mut stderr).take(4096).read_to_end(&mut kept);
        // Whatever else it writes is read and dropped, so it never blocks on a full pipe
        let _ = std::io::copy(&mut stderr, &mut std::io::sink());
        kept
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
            None if too_big.load(std::sync::atomic::Ordering::SeqCst) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{program} wrote more than {MAX_OUTPUT} bytes"));
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    };
    let output = reader
        .join()
        .map_err(|_| "reader thread panicked")?
        .map_err(|e| e.to_string())?;
    let errors = errors.join().unwrap_or_default();
    // A listing cut short, or an image half written, must not be taken for the whole
    if output.len() as u64 > MAX_OUTPUT {
        return Err(format!("{program} wrote more than {MAX_OUTPUT} bytes"));
    }
    if !status.success() {
        let errors = String::from_utf8_lossy(&errors);
        tracing::debug!("{program} failed with {status}: {}", errors.trim());
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
        // A damaged file has no first page to draw a cover from
        cover: info
            .lines()
            .any(|l| l.starts_with("Pages:"))
            .then_some(CoverSource::Pdf),
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

/// bsdtar writes names the way the locale it runs in allows: in a UTF-8 one as they are, in the
/// plain C one (a service started without `LANG`) every character outside ASCII as `\ooo`.
const UTF8_LOCALE: [(&str, &str); 2] = [("LC_ALL", "C.UTF-8"), ("LANG", "C.UTF-8")];

/// Turns the `\ooo` escapes bsdtar writes for bytes it cannot show (a name that is not UTF-8,
/// such as Latin-1 ones) back into those bytes.
fn unescape_octal(name: &str) -> Vec<u8> {
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let digits = bytes
            .get(i + 1..i + 4)
            .filter(|d| d.iter().all(|b| (b'0'..=b'7').contains(b)));
        match (bytes[i], digits) {
            (b'\\', Some(d)) if d[0] <= b'3' => {
                out.push((d[0] - b'0') * 64 + (d[1] - b'0') * 8 + (d[2] - b'0'));
                i += 4;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    out
}

pub fn archive_entries(path: &Path) -> Result<Vec<String>, String> {
    let list = run_with("bsdtar", &["-tf".as_ref(), path.as_os_str()], &UTF8_LOCALE)?;
    Ok(String::from_utf8_lossy(&list)
        .lines()
        .filter(|l| !l.is_empty() && !l.ends_with('/'))
        .map(str::to_string)
        .collect())
}

/// bsdtar treats member names as glob patterns, so escape the special characters.
fn escape_pattern(name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len());
    for &b in name {
        if matches!(b, b'\\' | b'*' | b'?' | b'[' | b']') {
            out.push(b'\\');
        }
        out.push(b);
    }
    out
}

pub fn archive_entry(path: &Path, entry: &str) -> Result<Vec<u8>, String> {
    let pattern = escape_pattern(&unescape_octal(entry));
    #[cfg(unix)]
    let pattern = <std::ffi::OsString as std::os::unix::ffi::OsStringExt>::from_vec(pattern);
    #[cfg(not(unix))]
    let pattern = String::from_utf8_lossy(&pattern).into_owned();
    let data = run_with(
        "bsdtar",
        &["-xOf".as_ref(), path.as_os_str(), "--".as_ref(), pattern.as_ref()],
        &UTF8_LOCALE,
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
    #[test]
    fn the_escapes_of_bsdtar_give_back_the_bytes_of_a_name() {
        // "Capítulo" in UTF-8, as bsdtar writes it in the plain C locale
        assert_eq!(
            unescape_octal("Cap\\303\\255tulo 1/001.jpg"),
            "Capítulo 1/001.jpg".as_bytes()
        );
        // A Latin-1 "é" stays one byte that is not UTF-8
        assert_eq!(unescape_octal("Caf\\351.jpg"), b"Caf\xe9.jpg");
        // Not an escape: a backslash, or digits that are not octal
        assert_eq!(unescape_octal("a\\b\\789\\477"), b"a\\b\\789\\477");
        assert_eq!(escape_pattern(b"a*b[1]\\c"), b"a\\*b\\[1\\]\\\\c");
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_or_cut_short_command_is_an_error() {
        use std::ffi::OsStr;
        let sh = |script: &'static str| run("sh", &[OsStr::new("-c"), OsStr::new(script)]);
        assert_eq!(sh("printf abc").unwrap(), b"abc");
        // Output and then a failure: what came out may be half of it
        assert!(sh("printf abc; echo broken >&2; exit 3")
            .unwrap_err()
            .contains("failed"));
        // More than is allowed is not silently cut
        let big = sh("head -c 70000000 /dev/zero");
        assert!(big.unwrap_err().contains("more than"));
    }

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
