//! Data types for Thingiverse API responses.
//!
//! The API is inconsistent: fields go missing, ids and counts arrive as either
//! numbers or strings, and nulls appear anywhere. Every field is therefore
//! optional and numeric fields go through a lenient deserializer.

use serde::{Deserialize, Deserializer, Serialize};

/// Accepts a number, a numeric string, or null/garbage (as `None`).
fn lenient_u64<'de, D: Deserializer<'de>>(d: D) -> Result<Option<u64>, D::Error> {
    let v = Option::<serde_json::Value>::deserialize(d)?;
    Ok(match v {
        // Whole-number floats such as `3.0` also occur; anything else is dropped.
        Some(serde_json::Value::Number(n)) => n.as_u64().or_else(|| {
            n.as_f64().filter(|f| f.fract() == 0.0 && *f >= 0.0).and_then(|f| format!("{f:.0}").parse().ok())
        }),
        Some(serde_json::Value::String(s)) => s.trim().parse().ok(),
        _ => None,
    })
}

/// Accepts a string or null; anything else (numbers, objects) becomes `None`
/// rather than failing the whole response.
fn lenient_string<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    let v = Option::<serde_json::Value>::deserialize(d)?;
    Ok(match v {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => Some(s),
        Some(serde_json::Value::Number(n)) => Some(n.to_string()),
        _ => None,
    })
}

/// Deserializes an object, falling back to `None` if it has the wrong shape.
fn lenient_obj<'de, D, T>(d: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let v = Option::<serde_json::Value>::deserialize(d)?;
    Ok(v.and_then(|v| serde_json::from_value(v).ok()))
}

/// Deserializes an array, skipping elements that do not parse.
pub(crate) fn lenient_vec<'de, D, T>(d: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let v = Option::<serde_json::Value>::deserialize(d)?;
    Ok(match v {
        Some(serde_json::Value::Array(items)) => {
            items.into_iter().filter_map(|i| serde_json::from_value(i).ok()).collect()
        }
        _ => Vec::new(),
    })
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct User {
    #[serde(deserialize_with = "lenient_u64")]
    pub id: Option<u64>,
    #[serde(deserialize_with = "lenient_string")]
    pub name: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub first_name: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub last_name: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub public_url: Option<String>,
}

impl User {
    /// Best human-readable name: "First Last", else the username.
    pub fn display_name(&self) -> Option<String> {
        let full = [self.first_name.as_deref(), self.last_name.as_deref()]
            .into_iter()
            .flatten()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        if full.is_empty() { self.name.clone() } else { Some(full) }
    }
}

/// A thing, as returned both by search hits and by `GET /things/{id}`.
/// Search hits simply leave the detail fields empty.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Thing {
    #[serde(deserialize_with = "lenient_u64")]
    pub id: Option<u64>,
    #[serde(deserialize_with = "lenient_string")]
    pub name: Option<String>,
    #[serde(deserialize_with = "lenient_obj")]
    pub creator: Option<User>,
    #[serde(deserialize_with = "lenient_u64")]
    pub like_count: Option<u64>,
    #[serde(deserialize_with = "lenient_u64")]
    pub download_count: Option<u64>,
    #[serde(deserialize_with = "lenient_u64")]
    pub make_count: Option<u64>,
    #[serde(deserialize_with = "lenient_u64")]
    pub file_count: Option<u64>,
    #[serde(deserialize_with = "lenient_string")]
    pub public_url: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub thumbnail: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub added: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub license: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub description: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub description_html: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub instructions: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub instructions_html: Option<String>,
}

impl Thing {
    pub fn title(&self) -> String {
        match (&self.name, self.id) {
            (Some(n), _) => n.trim().to_string(),
            (None, Some(id)) => format!("Thing {id}"),
            (None, None) => "Untitled thing".to_string(),
        }
    }

    pub fn creator_name(&self) -> String {
        self.creator.as_ref().and_then(User::display_name).unwrap_or_else(|| "Unknown creator".to_string())
    }

    /// Web page for this thing, derived from the id if the API omitted it.
    pub fn web_url(&self) -> Option<String> {
        self.public_url.clone().or_else(|| self.id.map(|id| format!("https://www.thingiverse.com/thing:{id}")))
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ThingFile {
    #[serde(deserialize_with = "lenient_u64")]
    pub id: Option<u64>,
    #[serde(deserialize_with = "lenient_string")]
    pub name: Option<String>,
    #[serde(deserialize_with = "lenient_u64")]
    pub size: Option<u64>,
    #[serde(deserialize_with = "lenient_string")]
    pub formatted_size: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub download_url: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub direct_url: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub public_url: Option<String>,
    #[serde(deserialize_with = "lenient_u64")]
    pub download_count: Option<u64>,
    #[serde(deserialize_with = "lenient_string")]
    pub date: Option<String>,
}

impl ThingFile {
    pub fn display_name(&self) -> String {
        self.name
            .clone()
            .or_else(|| self.id.map(|id| format!("file-{id}")))
            .unwrap_or_else(|| "unnamed file".to_string())
    }

    pub fn size_text(&self) -> String {
        if let Some(s) = &self.formatted_size {
            return s.clone();
        }
        match self.size {
            Some(b) if b >= 1024 * 1024 => {
                format!("{}.{} MB", b / (1024 * 1024), b % (1024 * 1024) * 10 / (1024 * 1024))
            }
            Some(b) if b >= 1024 => format!("{}.{} kB", b / 1024, b % 1024 * 10 / 1024),
            Some(b) => format!("{b} bytes"),
            None => "unknown size".to_string(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ImageSize {
    #[serde(rename = "type", deserialize_with = "lenient_string")]
    pub kind: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub size: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Image {
    #[serde(deserialize_with = "lenient_u64")]
    pub id: Option<u64>,
    #[serde(deserialize_with = "lenient_string")]
    pub name: Option<String>,
    #[serde(deserialize_with = "lenient_string")]
    pub url: Option<String>,
    #[serde(deserialize_with = "lenient_vec")]
    pub sizes: Vec<ImageSize>,
}

impl Image {
    /// URL of the largest "display" rendition, falling back to the original.
    pub fn best_url(&self) -> Option<String> {
        let pick = |kind: &str, size: &str| {
            self.sizes
                .iter()
                .find(|s| s.kind.as_deref() == Some(kind) && s.size.as_deref() == Some(size))
                .and_then(|s| s.url.clone())
        };
        pick("display", "large").or_else(|| self.url.clone()).or_else(|| pick("preview", "large"))
    }
}

/// One page of search results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SearchPage {
    pub term: String,
    pub page: u32,
    pub per_page: u32,
    /// Total number of matches reported by the server, if it said.
    pub total: Option<u64>,
    pub hits: Vec<Thing>,
}

impl SearchPage {
    pub fn has_more(&self) -> bool {
        match self.total {
            Some(t) => u64::from(self.page) * u64::from(self.per_page) < t,
            None => u32::try_from(self.hits.len()).unwrap_or(u32::MAX) >= self.per_page,
        }
    }
}

/// Everything the details view needs, fetched together.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThingDetails {
    pub thing: Thing,
    pub files: Vec<ThingFile>,
    pub images: Vec<Image>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerates_string_numbers_and_nulls() {
        let t: Thing =
            serde_json::from_str(r#"{"id":"763622","name":null,"like_count":"12","creator":"oops","file_count":3.0}"#)
                .unwrap();
        assert_eq!(t.id, Some(763_622));
        assert_eq!(t.like_count, Some(12));
        assert_eq!(t.file_count, Some(3));
        assert!(t.creator.is_none());
        assert_eq!(t.title(), "Thing 763622");
        assert_eq!(t.web_url().unwrap(), "https://www.thingiverse.com/thing:763622");
    }

    #[test]
    fn creator_display_name_prefers_full_name() {
        let u: User = serde_json::from_str(r#"{"name":"cts","first_name":"Creative","last_name":" Tools "}"#).unwrap();
        assert_eq!(u.display_name().unwrap(), "Creative Tools");
        let u: User = serde_json::from_str(r#"{"name":"cts","first_name":""}"#).unwrap();
        assert_eq!(u.display_name().unwrap(), "cts");
    }

    #[test]
    fn image_best_url() {
        let i: Image = serde_json::from_str(
            r#"{"url":"orig","sizes":[{"type":"thumb","size":"large","url":"t"},{"type":"display","size":"large","url":"d"},5]}"#,
        )
        .unwrap();
        assert_eq!(i.sizes.len(), 2);
        assert_eq!(i.best_url().unwrap(), "d");
    }
}
