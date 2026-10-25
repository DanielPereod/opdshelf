//! Looks up book metadata by ISBN on Open Library, falling back to Google Books.

use serde::Serialize;
use serde_json::Value;
use std::time::Duration;

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct Found {
    pub source: String,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub publisher: Option<String>,
    pub date: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    pub subjects: Vec<String>,
    pub identifier: String,
    #[serde(rename = "coverUrl")]
    pub cover_url: Option<String>,
}

/// Hosts covers may be downloaded from (the lookup results only point to these).
pub const COVER_HOSTS: &[&str] = &[
    "covers.openlibrary.org",
    "books.google.com",
    "books.googleusercontent.com",
];

#[derive(Clone)]
pub struct Lookup {
    client: reqwest::Client,
    openlibrary_url: String,
    google_books_url: String,
}

/// Keeps digits (and a final X) so "978-84-376-0494-7" becomes "9788437604947".
pub fn normalize_isbn(input: &str) -> Option<String> {
    let isbn: String = input
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let valid = match isbn.len() {
        10 => isbn[..9].chars().all(|c| c.is_ascii_digit()),
        13 => isbn.chars().all(|c| c.is_ascii_digit()),
        _ => false,
    };
    valid.then_some(isbn)
}

fn text(value: &Value) -> Option<String> {
    value.as_str().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

impl Lookup {
    pub fn new(openlibrary_url: &str, google_books_url: &str) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .user_agent(concat!(
                "OPDShelf/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/DanielPereod/opdshelf)"
            ))
            .build()
            .expect("HTTP client");
        Self {
            client,
            openlibrary_url: openlibrary_url.trim_end_matches('/').to_string(),
            google_books_url: google_books_url.trim_end_matches('/').to_string(),
        }
    }

    pub async fn isbn(&self, isbn: &str) -> Result<Option<Found>, String> {
        let isbn = normalize_isbn(isbn).ok_or("Invalid ISBN")?;
        let open_library = self.open_library(&isbn).await;
        if let Ok(Some(found)) = &open_library {
            return Ok(Some(found.clone()));
        }
        match self.google_books(&isbn).await {
            Ok(found) => Ok(found),
            Err(err) => open_library.and(Err(err)),
        }
    }

    async fn get_json(&self, url: &str) -> Result<Value, String> {
        let response = self.client.get(url).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("{url}: HTTP {}", response.status()));
        }
        response.json().await.map_err(|e| e.to_string())
    }

    async fn open_library(&self, isbn: &str) -> Result<Option<Found>, String> {
        let url = format!(
            "{}/api/books?bibkeys=ISBN:{isbn}&format=json&jscmd=data",
            self.openlibrary_url
        );
        let json = self.get_json(&url).await?;
        let Some(book) = json.get(format!("ISBN:{isbn}")) else {
            return Ok(None);
        };
        let names = |key: &str| -> Vec<String> {
            book[key]
                .as_array()
                .map(|items| items.iter().filter_map(|i| text(&i["name"])).collect())
                .unwrap_or_default()
        };
        let mut description = text(&book["notes"]).or_else(|| text(&book["notes"]["value"]));
        // The data API rarely has a description; the edition record sometimes does
        if description.is_none() {
            if let Ok(edition) = self
                .get_json(&format!("{}/isbn/{isbn}.json", self.openlibrary_url))
                .await
            {
                description = text(&edition["description"]).or_else(|| text(&edition["description"]["value"]));
            }
        }
        Ok(Some(Found {
            source: "Open Library".into(),
            title: text(&book["title"]).map(|t| match text(&book["subtitle"]) {
                Some(sub) => format!("{t}: {sub}"),
                None => t,
            }),
            authors: names("authors"),
            publisher: names("publishers").into_iter().next(),
            date: text(&book["publish_date"]),
            description,
            language: None,
            subjects: names("subjects").into_iter().take(8).collect(),
            identifier: isbn.to_string(),
            cover_url: text(&book["cover"]["large"]).or_else(|| text(&book["cover"]["medium"])),
        }))
    }

    async fn google_books(&self, isbn: &str) -> Result<Option<Found>, String> {
        let url = format!("{}/books/v1/volumes?q=isbn:{isbn}", self.google_books_url);
        let json = self.get_json(&url).await?;
        let Some(info) = json["items"].get(0).map(|item| &item["volumeInfo"]) else {
            return Ok(None);
        };
        let list = |key: &str| -> Vec<String> {
            info[key]
                .as_array()
                .map(|items| items.iter().filter_map(text).collect())
                .unwrap_or_default()
        };
        let cover_url = text(&info["imageLinks"]["thumbnail"])
            .or_else(|| text(&info["imageLinks"]["smallThumbnail"]))
            .map(|url| url.replacen("http://", "https://", 1).replace("&edge=curl", ""));
        Ok(Some(Found {
            source: "Google Books".into(),
            title: text(&info["title"]).map(|t| match text(&info["subtitle"]) {
                Some(sub) => format!("{t}: {sub}"),
                None => t,
            }),
            authors: list("authors"),
            publisher: text(&info["publisher"]),
            date: text(&info["publishedDate"]),
            description: text(&info["description"]).map(|d| crate::metadata::html_to_text(&d)),
            language: text(&info["language"]),
            subjects: list("categories"),
            identifier: isbn.to_string(),
            cover_url,
        }))
    }

    /// Downloads a cover found by a lookup. Only the known cover hosts are allowed,
    /// so the server cannot be used to fetch arbitrary URLs.
    pub async fn download_cover(&self, url: &str) -> Result<Vec<u8>, String> {
        let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
        let host = parsed.host_str().unwrap_or_default();
        let allowed = parsed.scheme() == "https" && COVER_HOSTS.contains(&host)
            || url.starts_with(&format!("{}/", self.openlibrary_url))
            || url.starts_with(&format!("{}/", self.google_books_url));
        if !allowed {
            return Err(format!("covers cannot be downloaded from {host}"));
        }
        let response = self.client.get(parsed).send().await.map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("HTTP {}", response.status()));
        }
        let bytes = response.bytes().await.map_err(|e| e.to_string())?;
        if bytes.len() > 20 * 1024 * 1024 {
            return Err("cover too large".into());
        }
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn isbn_normalization() {
        assert_eq!(normalize_isbn("978-84-376-0494-7").as_deref(), Some("9788437604947"));
        assert_eq!(normalize_isbn("0-306-40615-x").as_deref(), Some("030640615X"));
        assert_eq!(
            normalize_isbn("urn:isbn:9788437604947").as_deref(),
            Some("9788437604947")
        );
        assert_eq!(normalize_isbn("12345"), None);
        assert_eq!(normalize_isbn("urn:uuid:1234"), None);
    }

    #[tokio::test]
    async fn refuses_unknown_cover_hosts() {
        let lookup = Lookup::new("https://openlibrary.org", "https://www.googleapis.com");
        for url in [
            "http://169.254.169.254/latest",
            "https://evil.example/x.jpg",
            "http://covers.openlibrary.org/b/id/1-L.jpg",
            "file:///etc/passwd",
        ] {
            assert!(lookup.download_cover(url).await.is_err(), "{url}");
        }
    }
}
