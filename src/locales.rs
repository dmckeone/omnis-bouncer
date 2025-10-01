use http::HeaderMap;
use http::header::ACCEPT_LANGUAGE;
use std::collections::HashSet;
use std::str::FromStr;
use tracing::error;

#[derive(Debug)]
pub enum Error {
    MissingValue,
}

// https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Accept-Language
pub struct AcceptLanguage {
    pub locale: String,
    pub quality: f32,
}

impl Eq for AcceptLanguage {}

impl PartialEq for AcceptLanguage {
    fn eq(&self, other: &Self) -> bool {
        self.quality == other.quality && self.locale.eq(&other.locale)
    }
}

impl PartialOrd for AcceptLanguage {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AcceptLanguage {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.quality > other.quality {
            std::cmp::Ordering::Greater
        } else if self.quality < other.quality {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    }
}

impl FromStr for AcceptLanguage {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut locale = s.trim().split(';');
        let (locale, quality) = (locale.next(), locale.next());

        let locale = match locale {
            Some(locale) if !locale.is_empty() => locale,
            _ => return Err(Error::MissingValue),
        };

        let quality = match quality {
            Some(q) => match q.strip_prefix("q=") {
                Some(q) => q.parse::<f32>().unwrap_or(0.0),
                None => 1.0,
            },
            None => 1.0,
        };

        Ok(AcceptLanguage {
            locale: locale.to_lowercase(),
            quality,
        })
    }
}

// Given a string similar to the Accept-Language header, select a locale or return a reasonable
// default.
//
// https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Accept-Language
pub fn select_locale(
    accepted: impl Into<String>,
    permitted: &HashSet<String>,
    default: impl Into<String>,
) -> String {
    let accepted = accepted.into();
    let default = default.into();

    let mut accepted: Vec<AcceptLanguage> = accepted
        .split(',')
        .filter_map(|lang| match AcceptLanguage::from_str(lang) {
            Ok(accepted) => Some(accepted),
            Err(e) => match e {
                Error::MissingValue => None,
            },
        })
        .filter(|l| permitted.contains(&l.locale))
        .collect();

    accepted.sort(); // Ascending

    match accepted.iter().next_back() {
        Some(language) => language.locale.clone(),
        None => default,
    }
}

// Select a locale from the Accept-Language header of an HTTP request or return a reasonable
// default
//
// https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Accept-Language
pub fn header_locale(
    headers: &HeaderMap,
    permitted: &HashSet<String>,
    default: impl Into<String>,
) -> String {
    let default = default.into();
    match headers.get(ACCEPT_LANGUAGE) {
        Some(header) => match header.to_str() {
            Ok(accepted) => select_locale(accepted, permitted, &default),
            Err(error) => {
                error!("Failed to parse Accept-Language header: {}", error);
                default
            }
        },
        None => default,
    }
}

#[cfg(test)]
mod tests {
    use crate::locales::select_locale;
    use std::collections::HashSet;

    #[test]
    fn test_select_locale_default() {
        let default = "fr";
        let accepted = "en-CA,en-US;q=0.7,en;q=0.3";
        let permitted: HashSet<String> = vec!["fr"].into_iter().map(|s| s.into()).collect();
        assert_eq!(select_locale(accepted, &permitted, default), "fr")
    }

    #[test]
    fn test_select_locale_general() {
        let default = "fr";
        let accepted = "en-CA,en-US;q=0.7,en;q=0.3";
        let permitted: HashSet<String> = vec!["en"].into_iter().map(|s| s.into()).collect();
        assert_eq!(select_locale(accepted, &permitted, default), "en")
    }

    #[test]
    fn test_select_locale_specific() {
        let default = "fr";
        let accepted = "en-CA,en-US;q=0.7,en;q=0.3";
        let permitted: HashSet<String> =
            vec!["en", "en-us"].into_iter().map(|s| s.into()).collect();
        assert_eq!(select_locale(accepted, &permitted, default), "en-us")
    }
}
