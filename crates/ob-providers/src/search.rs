//! Web search clients: SearXNG (self-hosted) and Brave Search API.

use crate::client::{ProviderError, check};

const BRAVE_DEFAULT_BASE: &str = "https://api.search.brave.com";
const SNIPPET_MAX_CHARS: usize = 400;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchKind {
    Searxng,
    Brave,
}

pub struct SearchClient {
    kind: SearchKind,
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
}

impl SearchClient {
    pub fn new(
        kind: SearchKind,
        base_url: String,
        api_key: Option<String>,
    ) -> Result<Self, ProviderError> {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(8))
            .build()
            .map_err(|e| ProviderError::Connect(e.to_string()))?;
        Ok(Self {
            kind,
            base_url,
            api_key,
            http,
        })
    }

    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>, ProviderError> {
        let hits = match self.kind {
            SearchKind::Searxng => self.search_searxng(query).await?,
            SearchKind::Brave => self.search_brave(query, limit).await?,
        };
        Ok(hits.into_iter().take(limit).collect())
    }

    async fn search_searxng(&self, query: &str) -> Result<Vec<SearchHit>, ProviderError> {
        let base = self.base_url.trim_end_matches('/');
        let url = format!("{base}/search");
        let req = self
            .http
            .get(&url)
            .query(&[("q", query), ("format", "json")]);
        let resp = check(req.send().await, &url).await?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let results = body
            .get("results")
            .and_then(|r| r.as_array())
            .ok_or_else(|| ProviderError::BadResponse("missing results".to_string()))?;
        Ok(hits_from(results, "content"))
    }

    async fn search_brave(
        &self,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchHit>, ProviderError> {
        let api_key = self
            .api_key
            .as_ref()
            .filter(|k| !k.is_empty())
            .ok_or(ProviderError::Auth(401))?;
        let base = if self.base_url.trim().is_empty() {
            BRAVE_DEFAULT_BASE.to_string()
        } else {
            self.base_url.trim_end_matches('/').to_string()
        };
        let url = format!("{base}/res/v1/web/search");
        let count = limit.to_string();
        let req = self
            .http
            .get(&url)
            .header("X-Subscription-Token", api_key)
            .query(&[("q", query), ("count", count.as_str())]);
        let resp = check(req.send().await, &url).await?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let results = body
            .get("web")
            .and_then(|w| w.get("results"))
            .and_then(|r| r.as_array())
            .ok_or_else(|| ProviderError::BadResponse("missing web.results".to_string()))?;
        Ok(hits_from(results, "description"))
    }
}

fn hits_from(items: &[serde_json::Value], snippet_key: &str) -> Vec<SearchHit> {
    items
        .iter()
        .filter_map(|item| {
            let url = item.get("url")?.as_str()?.to_string();
            let field = |k: &str| {
                item.get(k)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            let snippet: String = field(snippet_key)
                .trim()
                .chars()
                .take(SNIPPET_MAX_CHARS)
                .collect();
            Some(SearchHit {
                title: field("title"),
                url,
                snippet,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::MockServer;

    #[tokio::test]
    async fn searxng_parses_two_hits_and_sends_format_json() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/searx/search")
                .query_param("q", "rust")
                .query_param("format", "json");
            then.status(200).json_body(serde_json::json!({
                "results": [
                    {"title": "A", "url": "http://a.example", "content": "  snip a  "},
                    {"title": "B", "url": "http://b.example", "content": "snip b"}
                ]
            }));
        });
        let client = SearchClient::new(SearchKind::Searxng, server.url("/searx/"), None).unwrap();
        let hits = client.search("rust", 5).await.unwrap();
        assert_eq!(
            hits,
            vec![
                SearchHit {
                    title: "A".to_string(),
                    url: "http://a.example".to_string(),
                    snippet: "snip a".to_string(),
                },
                SearchHit {
                    title: "B".to_string(),
                    url: "http://b.example".to_string(),
                    snippet: "snip b".to_string(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn brave_sends_token_header_and_parses() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::GET)
                .path("/res/v1/web/search")
                .header("x-subscription-token", "tok-1")
                .query_param("q", "crates")
                .query_param("count", "3");
            then.status(200).json_body(serde_json::json!({
                "web": {"results": [
                    {"title": "X", "url": "http://x.example", "description": "dx"}
                ]}
            }));
        });
        let client =
            SearchClient::new(SearchKind::Brave, server.url(""), Some("tok-1".to_string()))
                .unwrap();
        let hits = client.search("crates", 3).await.unwrap();
        assert_eq!(
            hits,
            vec![SearchHit {
                title: "X".to_string(),
                url: "http://x.example".to_string(),
                snippet: "dx".to_string(),
            }]
        );
    }

    #[tokio::test]
    async fn brave_without_key_is_auth_error() {
        let client = SearchClient::new(SearchKind::Brave, String::new(), None).unwrap();
        let err = client.search("q", 1).await.unwrap_err();
        assert!(matches!(err, ProviderError::Auth(401)));
    }

    #[tokio::test]
    async fn limit_truncates_hits() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/search");
            then.status(200).json_body(serde_json::json!({
                "results": [
                    {"title": "1", "url": "http://1", "content": "a"},
                    {"title": "2", "url": "http://2", "content": "b"},
                    {"title": "3", "url": "http://3", "content": "c"}
                ]
            }));
        });
        let client = SearchClient::new(SearchKind::Searxng, server.url(""), None).unwrap();
        let hits = client.search("q", 2).await.unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[1].url, "http://2");
    }

    #[tokio::test]
    async fn snippet_truncated_char_safe() {
        let server = MockServer::start_async().await;
        let long = "汉".repeat(500);
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/search");
            then.status(200).json_body(serde_json::json!({
                "results": [{"title": "t", "url": "http://u", "content": long}]
            }));
        });
        let client = SearchClient::new(SearchKind::Searxng, server.url(""), None).unwrap();
        let hits = client.search("q", 1).await.unwrap();
        assert_eq!(hits[0].snippet.chars().count(), 400);
    }

    #[tokio::test]
    async fn http_error_maps_via_shared_check() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/search");
            then.status(429);
        });
        let client = SearchClient::new(SearchKind::Searxng, server.url(""), None).unwrap();
        let err = client.search("q", 1).await.unwrap_err();
        assert!(matches!(err, ProviderError::RateLimited));
    }
}
