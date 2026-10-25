//! Metadata and cover extraction for EPUB, FB2 and comic archives.
//!
//! Parsing never reads a whole book into memory: archives are opened with
//! seeking and only the entries that are needed are decompressed.

use base64::Engine;
use percent_encoding::percent_decode_str;
use serde::Serialize;
use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;

use crate::paths;

const MAX_XML_BYTES: u64 = 16 * 1024 * 1024;
const MAX_FB2_BYTES: u64 = 64 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct BookMeta {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub subjects: Vec<String>,
    pub date: Option<String>,
    pub identifier: Option<String>,
    #[serde(skip)]
    pub cover: Option<CoverSource>,
}

/// Where the cover lives inside the book, found while reading the metadata.
#[derive(Clone, Debug, PartialEq)]
pub enum CoverSource {
    /// An image entry of a ZIP based book (EPUB, CBZ).
    ZipEntry { name: String, mime: String },
    /// A `<binary>` element of an FB2 file, optionally inside a ZIP (`.fb2.zip`).
    Fb2Binary {
        zip_entry: Option<String>,
        id: String,
        mime: String,
    },
}

impl CoverSource {
    pub fn mime(&self) -> &str {
        match self {
            CoverSource::ZipEntry { mime, .. } | CoverSource::Fb2Binary { mime, .. } => mime,
        }
    }
}

pub fn read_metadata(path: &Path) -> BookMeta {
    let ext = paths::extension(&path.to_string_lossy());
    let result = match ext.as_str() {
        "epub" | "kepub" => read_epub(path),
        "fb2" => read_fb2_file(path),
        "cbz" | "zip" | "fb2.zip" | "fbz" => read_zip_book(path),
        _ => Ok(BookMeta::default()),
    };
    result.unwrap_or_else(|err| {
        tracing::debug!("could not read metadata of {}: {err}", path.display());
        BookMeta::default()
    })
}

pub fn read_cover(path: &Path, cover: &CoverSource) -> Option<Vec<u8>> {
    let result = match cover {
        CoverSource::ZipEntry { name, .. } => {
            open_zip(path).and_then(|mut zip| read_entry(&mut zip, name, MAX_IMAGE_BYTES))
        }
        CoverSource::Fb2Binary { zip_entry, id, .. } => {
            let xml = match zip_entry {
                Some(entry) => open_zip(path).and_then(|mut zip| read_entry(&mut zip, entry, MAX_FB2_BYTES)),
                None => File::open(path)
                    .map_err(|e| e.to_string())
                    .and_then(|f| read_limited(f, MAX_FB2_BYTES)),
            };
            xml.and_then(|xml| fb2_binary(&decode_xml(&xml), id))
        }
    };
    result
        .map_err(|err| tracing::debug!("could not read cover of {}: {err}", path.display()))
        .ok()
}

type Zip = zip::ZipArchive<BufReader<File>>;

fn open_zip(path: &Path) -> Result<Zip, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    zip::ZipArchive::new(BufReader::new(file)).map_err(|e| e.to_string())
}

fn read_limited(reader: impl Read, limit: u64) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    reader
        .take(limit + 1)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    if buf.len() as u64 > limit {
        return Err("entry too large".into());
    }
    Ok(buf)
}

fn read_entry<R: Read + Seek>(zip: &mut zip::ZipArchive<R>, name: &str, limit: u64) -> Result<Vec<u8>, String> {
    let entry = zip.by_name(name).map_err(|e| format!("{name}: {e}"))?;
    read_limited(entry, limit)
}

fn image_mime(name: &str) -> Option<&'static str> {
    match paths::extension(name).as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// Resolves `href` relative to the folder `base`, as found in OPF manifests.
fn join_href(base: &str, href: &str) -> String {
    let href = percent_decode_str(href.split('#').next().unwrap_or(href)).decode_utf8_lossy();
    let mut parts: Vec<&str> = base.split('/').filter(|p| !p.is_empty()).collect();
    for part in href.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("/")
}

/// Turns descriptions that contain HTML into plain text with paragraph breaks.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut chars = html.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag = String::new();
            for c in chars.by_ref() {
                if c == '>' {
                    break;
                }
                tag.push(c);
            }
            let name = tag
                .trim_start_matches('/')
                .split(|c: char| c.is_whitespace() || c == '/')
                .next()
                .unwrap_or("")
                .to_lowercase();
            if matches!(
                name.as_str(),
                "br" | "p" | "div" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6"
            ) {
                out.push('\n');
            }
        } else {
            out.push(c);
        }
    }
    let text = decode_entities(&out);
    let lines: Vec<String> = text
        .lines()
        .map(|l| l.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    let mut result = String::new();
    for line in lines {
        if line.is_empty() {
            if !result.is_empty() && !result.ends_with("\n\n") {
                result.push('\n');
            }
            continue;
        }
        if !result.is_empty() && !result.ends_with('\n') {
            result.push('\n');
        }
        result.push_str(&line);
    }
    result.trim().to_string()
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest[..rest.len().min(12)].find(';');
        let decoded = end.and_then(|end| {
            let entity = &rest[1..end];
            let c = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                    u32::from_str_radix(&entity[2..], 16).ok().and_then(char::from_u32)
                }
                _ if entity.starts_with('#') => entity[1..].parse().ok().and_then(char::from_u32),
                _ => None,
            };
            c.map(|c| (c, end))
        });
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn clean(text: Option<&str>) -> Option<String> {
    text.map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
}

// ---------------------------------------------------------------------------
// EPUB

fn read_epub(path: &Path) -> Result<BookMeta, String> {
    let mut zip = open_zip(path)?;
    let container =
        String::from_utf8_lossy(&read_entry(&mut zip, "META-INF/container.xml", MAX_XML_BYTES)?).into_owned();
    let container = roxmltree::Document::parse(&container).map_err(|e| e.to_string())?;
    let opf_path = container
        .descendants()
        .find(|n| n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .ok_or("no rootfile in container.xml")?
        .to_string();

    let opf = String::from_utf8_lossy(&read_entry(&mut zip, &opf_path, MAX_XML_BYTES)?).into_owned();
    let mut meta = parse_opf(&opf, paths::parent(&opf_path))?;

    if meta
        .cover
        .as_ref()
        .is_some_and(|c| matches!(c, CoverSource::ZipEntry { name, .. } if zip.by_name(name).is_err()))
    {
        meta.cover = None;
    }
    if meta.cover.is_none() {
        meta.cover = find_zip_cover(&zip);
    }
    Ok(meta)
}

fn children_text<'a>(node: roxmltree::Node<'a, 'a>, name: &'static str) -> impl Iterator<Item = String> + 'a {
    node.children()
        .filter(move |n| n.is_element() && n.tag_name().name() == name)
        .filter_map(|n| clean(n.text()))
}

fn child<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> Option<roxmltree::Node<'a, 'a>> {
    node.children().find(|n| n.tag_name().name() == name)
}

fn parse_opf(opf: &str, base: &str) -> Result<BookMeta, String> {
    let doc = roxmltree::Document::parse(opf).map_err(|e| e.to_string())?;
    let metadata = doc
        .descendants()
        .find(|n| n.has_tag_name("metadata"))
        .ok_or("no metadata in OPF")?;
    let dc = |name: &'static str| children_text(metadata, name);

    let mut meta = BookMeta {
        title: dc("title").next(),
        authors: dc("creator").collect(),
        description: dc("description")
            .next()
            .map(|d| html_to_text(&d))
            .filter(|d| !d.is_empty()),
        language: dc("language").next(),
        publisher: dc("publisher").next(),
        subjects: dc("subject").collect(),
        date: dc("date").next(),
        identifier: dc("identifier")
            .find(|i| !i.starts_with("urn:uuid:"))
            .or_else(|| dc("identifier").next()),
        cover: None,
    };

    let items: Vec<_> = doc.descendants().filter(|n| n.has_tag_name("item")).collect();
    let cover_id = metadata
        .children()
        .find(|n| n.tag_name().name() == "meta" && n.attribute("name") == Some("cover"))
        .and_then(|n| n.attribute("content"));
    let is_image = |n: &roxmltree::Node| n.attribute("media-type").is_some_and(|m| m.starts_with("image/"));

    let cover_item = cover_id
        .and_then(|id| items.iter().find(|n| n.attribute("id") == Some(id)))
        .or_else(|| {
            items.iter().find(|n| {
                n.attribute("properties")
                    .is_some_and(|p| p.split_whitespace().any(|p| p == "cover-image"))
            })
        })
        .or_else(|| {
            items.iter().find(|n| {
                is_image(n)
                    && (n.attribute("id").is_some_and(|id| id.to_lowercase().contains("cover"))
                        || n.attribute("href").is_some_and(|h| h.to_lowercase().contains("cover")))
            })
        });

    if let Some(item) = cover_item {
        if let Some(href) = item.attribute("href") {
            let name = join_href(base, href);
            let mime = item
                .attribute("media-type")
                .filter(|m| m.starts_with("image/"))
                .map(str::to_string)
                .or_else(|| image_mime(&name).map(str::to_string));
            if let Some(mime) = mime {
                meta.cover = Some(CoverSource::ZipEntry { name, mime });
            }
        }
    }
    Ok(meta)
}

// ---------------------------------------------------------------------------
// Comics and zipped books

fn read_zip_book(path: &Path) -> Result<BookMeta, String> {
    let mut zip = open_zip(path)?;
    let fb2_entry = zip
        .file_names()
        .find(|n| n.to_lowercase().ends_with(".fb2"))
        .map(str::to_string);
    if let Some(entry) = fb2_entry {
        let xml = read_entry(&mut zip, &entry, MAX_FB2_BYTES)?;
        let mut meta = parse_fb2(&decode_xml(&xml))?;
        if let Some(CoverSource::Fb2Binary { zip_entry, .. }) = &mut meta.cover {
            *zip_entry = Some(entry);
        }
        return Ok(meta);
    }

    let mut meta = BookMeta::default();
    let comic_info = zip
        .file_names()
        .find(|n| n.eq_ignore_ascii_case("ComicInfo.xml"))
        .map(str::to_string);
    if let Some(xml) = comic_info {
        if let Ok(xml) = read_entry(&mut zip, &xml, MAX_XML_BYTES) {
            meta = parse_comic_info(&String::from_utf8_lossy(&xml)).unwrap_or_default();
        }
    }
    meta.cover = find_zip_cover(&zip);
    Ok(meta)
}

/// Picks `cover.*` if present, otherwise the first page in natural order.
fn find_zip_cover<R: Read + Seek>(zip: &zip::ZipArchive<R>) -> Option<CoverSource> {
    let mut images: Vec<&str> = zip
        .file_names()
        .filter(|n| image_mime(n).is_some() && !n.contains("__MACOSX") && !paths::file_name(n).starts_with('.'))
        .collect();
    images.sort_by(|a, b| natural_cmp(a, b));
    let name = images
        .iter()
        .find(|n| paths::stem(n).eq_ignore_ascii_case("cover"))
        .or_else(|| images.first())?
        .to_string();
    let mime = image_mime(&name)?.to_string();
    Some(CoverSource::ZipEntry { name, mime })
}

/// Compares strings treating runs of digits as numbers ("page2" < "page10").
pub fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut n = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        n.push(c);
                        it.next();
                    }
                    n
                };
                let (na, nb) = (take(&mut a), take(&mut b));
                let (ta, tb) = (na.trim_start_matches('0'), nb.trim_start_matches('0'));
                let ord = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
                a.next();
                b.next();
            }
        }
    }
}

fn parse_comic_info(xml: &str) -> Result<BookMeta, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| e.to_string())?;
    let field = |name: &str| clean(doc.descendants().find(|n| n.has_tag_name(name)).and_then(|n| n.text()));
    let title = match (field("Series"), field("Number"), field("Title")) {
        (Some(series), Some(number), _) => Some(format!("{series} #{number}")),
        (_, _, Some(title)) => Some(title),
        (Some(series), None, None) => Some(series),
        _ => None,
    };
    let authors = ["Writer", "Penciller"]
        .iter()
        .filter_map(|f| field(f))
        .flat_map(|f| f.split(',').map(|a| a.trim().to_string()).collect::<Vec<_>>())
        .filter(|a| !a.is_empty())
        .collect();
    Ok(BookMeta {
        title,
        authors,
        description: field("Summary").map(|s| html_to_text(&s)),
        language: field("LanguageISO"),
        publisher: field("Publisher"),
        subjects: field("Genre")
            .map(|g| g.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_default(),
        date: field("Year"),
        identifier: None,
        cover: None,
    })
}

// ---------------------------------------------------------------------------
// FB2

fn read_fb2_file(path: &Path) -> Result<BookMeta, String> {
    let xml = read_limited(File::open(path).map_err(|e| e.to_string())?, MAX_FB2_BYTES)?;
    parse_fb2(&decode_xml(&xml))
}

/// FB2 files are often encoded in windows-1251 or similar; honour the XML declaration.
fn decode_xml(bytes: &[u8]) -> String {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(200)]).to_lowercase();
    let label = head
        .split("encoding=")
        .nth(1)
        .and_then(|rest| rest.trim_start_matches(['"', '\'']).split(['"', '\'']).next())
        .map(str::to_string);
    let encoding = label
        .and_then(|l| encoding_rs::Encoding::for_label(l.as_bytes()))
        .unwrap_or(encoding_rs::UTF_8);
    let (text, _, _) = encoding.decode(bytes);
    // roxmltree only accepts UTF-8, so the declaration must not claim otherwise
    match text.find("?>") {
        Some(end) if text.trim_start().starts_with("<?xml") => text[end + 2..].to_string(),
        _ => text.into_owned(),
    }
}

fn parse_fb2(xml: &str) -> Result<BookMeta, String> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(xml, options).map_err(|e| e.to_string())?;
    let title_info = doc
        .descendants()
        .find(|n| n.has_tag_name("title-info"))
        .ok_or("no title-info")?;
    let text_of = |node: Option<roxmltree::Node>| {
        clean(Some(
            &node?
                .descendants()
                .filter(|n| n.is_text())
                .map(|n| n.text().unwrap_or(""))
                .collect::<Vec<_>>()
                .join(" "),
        ))
    };

    let authors = title_info
        .children()
        .filter(|n| n.tag_name().name() == "author")
        .filter_map(|a| {
            let parts: Vec<String> = ["first-name", "middle-name", "last-name"]
                .iter()
                .filter_map(|p| text_of(child(a, p)))
                .collect();
            if parts.is_empty() {
                text_of(child(a, "nickname"))
            } else {
                Some(parts.join(" "))
            }
        })
        .collect();

    let description = child(title_info, "annotation").map(|a| {
        a.children()
            .filter(|n| n.is_element())
            .filter_map(|p| text_of(Some(p)))
            .collect::<Vec<_>>()
            .join("\n")
    });

    let publish_info = doc.descendants().find(|n| n.has_tag_name("publish-info"));
    let cover = child(title_info, "coverpage")
        .and_then(|c| c.descendants().find(|n| n.tag_name().name() == "image"))
        .and_then(|img| {
            img.attributes()
                .find(|a| a.name() == "href")
                .map(|a| a.value().trim_start_matches('#').to_string())
        })
        .and_then(|id| {
            let binary = doc
                .descendants()
                .find(|n| n.tag_name().name() == "binary" && n.attribute("id") == Some(id.as_str()))?;
            let mime = binary
                .attribute("content-type")
                .map(str::to_string)
                .or_else(|| image_mime(&id).map(str::to_string))?;
            Some(CoverSource::Fb2Binary {
                zip_entry: None,
                id,
                mime,
            })
        });

    Ok(BookMeta {
        title: text_of(child(title_info, "book-title")),
        authors,
        description: description.filter(|d| !d.is_empty()),
        language: text_of(child(title_info, "lang")),
        publisher: publish_info.and_then(|p| text_of(child(p, "publisher"))),
        subjects: title_info
            .children()
            .filter(|n| n.tag_name().name() == "genre")
            .filter_map(|g| text_of(Some(g)))
            .collect(),
        date: text_of(child(title_info, "date")).or_else(|| publish_info.and_then(|p| text_of(child(p, "year")))),
        identifier: publish_info.and_then(|p| text_of(child(p, "isbn"))),
        cover,
    })
}

fn fb2_binary(xml: &str, id: &str) -> Result<Vec<u8>, String> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(xml, options).map_err(|e| e.to_string())?;
    let data = doc
        .descendants()
        .find(|n| n.tag_name().name() == "binary" && n.attribute("id") == Some(id))
        .and_then(|n| n.text())
        .ok_or("binary not found")?;
    let data: String = data.chars().filter(|c| !c.is_whitespace()).collect();
    base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::io::Write;

    pub fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
        let mut zip = zip::ZipWriter::new(File::create(path).unwrap());
        for (name, data) in entries {
            zip.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            zip.write_all(data).unwrap();
        }
        zip.finish().unwrap();
    }

    pub fn write_epub(path: &Path, metadata: &str, manifest: &str, files: &[(&str, &[u8])]) {
        let opf = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" version="3.0">
<metadata>{metadata}</metadata><manifest>{manifest}</manifest></package>"#
        );
        let mut entries: Vec<(&str, &[u8])> = vec![
            ("mimetype", b"application/epub+zip"),
            ("META-INF/container.xml", br#"<?xml version="1.0"?><container xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#),
            ("OEBPS/content.opf", opf.as_bytes()),
        ];
        entries.extend_from_slice(files);
        write_zip(path, &entries);
    }

    #[test]
    fn epub_metadata_and_epub2_cover() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.epub");
        write_epub(
            &path,
            r#"<dc:title>El Quijote</dc:title><dc:creator>Miguel de Cervantes</dc:creator><dc:creator>Otro</dc:creator>
               <dc:language>es</dc:language><dc:identifier>urn:uuid:1</dc:identifier><dc:identifier>978-84</dc:identifier>
               <dc:description>&lt;p&gt;Un &lt;b&gt;hidalgo&lt;/b&gt; &amp;amp; su escudero&lt;/p&gt;&lt;p&gt;Segunda&lt;/p&gt;</dc:description>
               <meta name="cover" content="img"/>"#,
            r#"<item id="img" href="../Images/Portada%201.jpg" media-type="image/jpeg"/>"#,
            &[("Images/Portada 1.jpg", b"JPEG")],
        );
        let meta = read_metadata(&path);
        assert_eq!(meta.title.as_deref(), Some("El Quijote"));
        assert_eq!(meta.authors, vec!["Miguel de Cervantes", "Otro"]);
        assert_eq!(meta.language.as_deref(), Some("es"));
        assert_eq!(meta.identifier.as_deref(), Some("978-84"));
        assert_eq!(meta.description.as_deref(), Some("Un hidalgo & su escudero\nSegunda"));
        let cover = meta.cover.unwrap();
        assert_eq!(
            cover,
            CoverSource::ZipEntry {
                name: "Images/Portada 1.jpg".into(),
                mime: "image/jpeg".into()
            }
        );
        assert_eq!(read_cover(&path, &cover).unwrap(), b"JPEG");
    }

    #[test]
    fn epub3_cover_and_missing_cover() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.epub");
        write_epub(
            &path,
            "<dc:title>T</dc:title>",
            r#"<item id="c" properties="cover-image" href="c.png" media-type="image/png"/>"#,
            &[("OEBPS/c.png", b"PNG")],
        );
        assert_eq!(read_metadata(&path).cover.unwrap().mime(), "image/png");

        write_epub(&path, "<dc:title>T</dc:title>", "", &[]);
        let meta = read_metadata(&path);
        assert_eq!(meta.title.as_deref(), Some("T"));
        assert_eq!(meta.cover, None);
    }

    #[test]
    fn cbz_first_page_in_natural_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("c.cbz");
        write_zip(
            &path,
            &[
                ("p10.jpg", b"10"),
                ("p2.jpg", b"2"),
                ("__MACOSX/p1.jpg", b"x"),
                (
                    "ComicInfo.xml",
                    b"<ComicInfo><Series>Saga</Series><Number>3</Number><Writer>BKV</Writer></ComicInfo>",
                ),
            ],
        );
        let meta = read_metadata(&path);
        assert_eq!(meta.title.as_deref(), Some("Saga #3"));
        assert_eq!(meta.authors, vec!["BKV"]);
        assert_eq!(read_cover(&path, &meta.cover.unwrap()).unwrap(), b"2");
    }

    const FB2: &str = r##"<?xml version="1.0" encoding="windows-1251"?>
<FictionBook xmlns="http://www.gribuser.ru/xml/fictionbook/2.0" xmlns:l="http://www.w3.org/1999/xlink">
<description><title-info><genre>sf</genre><author><first-name>Ivan</first-name><last-name>Petrov</last-name></author>
<book-title>TITLE</book-title><annotation><p>Uno</p><p>Dos</p></annotation><lang>ru</lang>
<coverpage><image l:href="#cover.jpg"/></coverpage></title-info></description>
<body><section><p>text</p></section></body>
<binary id="cover.jpg" content-type="image/jpeg">SlBFRw==</binary></FictionBook>"##;

    #[test]
    fn fb2_in_legacy_encoding() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.fb2");
        let xml = FB2.replace("TITLE", "Пикник");
        let (bytes, _, _) = encoding_rs::WINDOWS_1251.encode(&xml);
        std::fs::write(&path, &bytes).unwrap();
        let meta = read_metadata(&path);
        assert_eq!(meta.title.as_deref(), Some("Пикник"));
        assert_eq!(meta.authors, vec!["Ivan Petrov"]);
        assert_eq!(meta.description.as_deref(), Some("Uno\nDos"));
        assert_eq!(meta.subjects, vec!["sf"]);
        assert_eq!(read_cover(&path, &meta.cover.unwrap()).unwrap(), b"JPEG");
    }

    #[test]
    fn zipped_fb2() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.fb2.zip");
        let xml = FB2.replace("windows-1251", "utf-8").replace("TITLE", "Zipped");
        write_zip(&path, &[("b.fb2", xml.as_bytes())]);
        let meta = read_metadata(&path);
        assert_eq!(meta.title.as_deref(), Some("Zipped"));
        assert_eq!(read_cover(&path, &meta.cover.unwrap()).unwrap(), b"JPEG");
    }

    #[test]
    fn broken_files_do_not_fail() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("b.epub");
        std::fs::write(&path, b"not a zip").unwrap();
        assert_eq!(read_metadata(&path), BookMeta::default());
    }

    #[test]
    fn natural_order() {
        let mut v = vec!["p10", "P2", "p1", "a"];
        v.sort_by(|a, b| natural_cmp(a, b));
        assert_eq!(v, vec!["a", "p1", "P2", "p10"]);
    }
}
