use reqwest::multipart::{Form, Part};

#[derive(Clone)]
pub struct ClientConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: Option<String>,
    pub connect_timeout_ms: u64,
    pub total_timeout_ms: u64,
}

impl std::fmt::Debug for ClientConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClientConfig")
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_ref().map(|_| "***"))
            .field("connect_timeout_ms", &self.connect_timeout_ms)
            .field("total_timeout_ms", &self.total_timeout_ms)
            .finish()
    }
}

impl ClientConfig {
    pub fn url(&self, route: &str) -> String {
        let base = self.base_url.trim_end_matches('/');
        let route = route.trim_start_matches('/');
        format!("{base}/{route}")
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("connect: {0}")]
    Connect(String),
    #[error("timeout")]
    Timeout,
    #[error("unauthorized ({0})")]
    Auth(u16),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("rate limited")]
    RateLimited,
    #[error("http {status}: {body}")]
    Http { status: u16, body: String },
    #[error("bad response: {0}")]
    BadResponse(String),
}

pub(crate) async fn check(
    res: Result<reqwest::Response, reqwest::Error>,
    url: &str,
) -> Result<reqwest::Response, ProviderError> {
    let resp = match res {
        Ok(resp) if resp.status().is_success() => return Ok(resp),
        Ok(resp) => resp,
        Err(e) => {
            if e.is_timeout() {
                return Err(ProviderError::Timeout);
            }
            if e.is_connect() {
                return Err(ProviderError::Connect(e.to_string()));
            }
            return Err(ProviderError::BadResponse(e.to_string()));
        }
    };
    let code = resp.status().as_u16();
    match code {
        401 | 403 => Err(ProviderError::Auth(code)),
        404 => Err(ProviderError::NotFound(url.to_string())),
        429 => Err(ProviderError::RateLimited),
        _ => {
            let body = resp.text().await.unwrap_or_default();
            let body: String = body.chars().take(500).collect();
            Err(ProviderError::Http { status: code, body })
        }
    }
}

pub struct OpenAiClient {
    cfg: ClientConfig,
    http: reqwest::Client,
}

impl OpenAiClient {
    pub fn new(cfg: ClientConfig) -> Result<Self, ProviderError> {
        let http = reqwest::Client::builder()
            .connect_timeout(std::time::Duration::from_millis(cfg.connect_timeout_ms))
            .timeout(std::time::Duration::from_millis(cfg.total_timeout_ms))
            .build()
            .map_err(|e| ProviderError::Connect(e.to_string()))?;
        Ok(Self { cfg, http })
    }

    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        match &self.cfg.api_key {
            Some(key) => req.bearer_auth(key),
            None => req,
        }
    }

    pub async fn list_models(&self) -> Result<Vec<String>, ProviderError> {
        let url = self.cfg.url("models");
        let req = self.http.get(&url);
        let req = self.apply_auth(req);
        let resp = check(req.send().await, &url).await?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let data = body
            .get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| ProviderError::BadResponse("missing data".to_string()))?;
        let ids = data
            .iter()
            .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
            .collect();
        Ok(ids)
    }

    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        max_tokens: u32,
        temperature: f32,
    ) -> Result<String, ProviderError> {
        self.chat_with(messages, max_tokens, temperature, false)
            .await
    }

    pub async fn chat_with(
        &self,
        messages: &[ChatMessage],
        max_tokens: u32,
        temperature: f32,
        disable_thinking: bool,
    ) -> Result<String, ProviderError> {
        let url = self.cfg.url("chat/completions");
        let mut payload = serde_json::json!({
            "model": self.cfg.model,
            "messages": messages,
            "max_tokens": max_tokens,
            "temperature": temperature,
            "stream": false,
        });
        if disable_thinking {
            payload["chat_template_kwargs"] = serde_json::json!({ "enable_thinking": false });
        }
        let req = self.http.post(&url).json(&payload);
        let req = self.apply_auth(req);
        let resp = check(req.send().await, &url).await?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let content = body
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| ProviderError::BadResponse("empty content".to_string()))?;
        Ok(content.to_string())
    }

    pub async fn transcribe(
        &self,
        wav: Vec<u8>,
        language: Option<&str>,
    ) -> Result<String, ProviderError> {
        self.transcribe_with_prompt(wav, language, None).await
    }

    pub async fn transcribe_with_prompt(
        &self,
        wav: Vec<u8>,
        language: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<String, ProviderError> {
        let url = self.cfg.url("audio/transcriptions");
        let part = Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let mut form = Form::new()
            .part("file", part)
            .text("model", self.cfg.model.clone())
            .text("response_format", "json".to_string());
        if let Some(lang) = language {
            form = form.text("language", lang.to_string());
        }
        if let Some(prompt) = prompt.filter(|p| !p.is_empty()) {
            form = form.text("prompt", prompt.to_string());
        }
        let req = self.http.post(&url).multipart(form);
        let req = self.apply_auth(req);
        let resp = check(req.send().await, &url).await?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| ProviderError::BadResponse(e.to_string()))?;
        let text = body
            .get("text")
            .and_then(|t| t.as_str())
            .ok_or_else(|| ProviderError::BadResponse("missing text".to_string()))?;
        Ok(text.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use httpmock::MockServer;

    fn cfg(server: &MockServer) -> ClientConfig {
        ClientConfig {
            base_url: server.url("/v1"),
            model: "test-model".to_string(),
            api_key: Some("k1".to_string()),
            connect_timeout_ms: 1000,
            total_timeout_ms: 5000,
        }
    }

    #[test]
    fn url_trims_slashes() {
        let c = ClientConfig {
            base_url: "http://example.com/v1/".to_string(),
            model: "m".to_string(),
            api_key: None,
            connect_timeout_ms: 0,
            total_timeout_ms: 0,
        };
        assert_eq!(c.url("/models"), "http://example.com/v1/models");
        assert_eq!(c.url("models"), "http://example.com/v1/models");
    }

    #[tokio::test]
    async fn list_models_parses_ids() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::GET).path("/v1/models");
            then.status(200).json_body(serde_json::json!({
                "data": [{"id": "a"}, {"id": "b"}]
            }));
        });
        let client = OpenAiClient::new(cfg(&server)).unwrap();
        let ids = client.list_models().await.unwrap();
        assert_eq!(ids, vec!["a".to_string(), "b".to_string()]);
    }

    #[tokio::test]
    async fn chat_returns_content_with_bearer() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/v1/chat/completions")
                .header("authorization", "Bearer k1");
            then.status(200).json_body(serde_json::json!({
                "choices": [{"message": {"content": "hello"}}]
            }));
        });
        let client = OpenAiClient::new(cfg(&server)).unwrap();
        let msgs = [ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let out = client.chat(&msgs, 100, 0.5).await.unwrap();
        assert_eq!(out, "hello");
    }

    #[tokio::test]
    async fn chat_unauthorized_maps_to_auth() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/v1/chat/completions");
            then.status(401);
        });
        let client = OpenAiClient::new(cfg(&server)).unwrap();
        let msgs = [ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let err = client.chat(&msgs, 100, 0.5).await.unwrap_err();
        assert!(matches!(err, ProviderError::Auth(401)));
    }

    #[tokio::test]
    async fn chat_rate_limited() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/v1/chat/completions");
            then.status(429);
        });
        let client = OpenAiClient::new(cfg(&server)).unwrap();
        let msgs = [ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let err = client.chat(&msgs, 100, 0.5).await.unwrap_err();
        assert!(matches!(err, ProviderError::RateLimited));
    }

    #[tokio::test]
    async fn transcribe_returns_trimmed_text() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/v1/audio/transcriptions")
                .body_contains("audio.wav");
            then.status(200)
                .json_body(serde_json::json!({"text": "  yes  "}));
        });
        let client = OpenAiClient::new(cfg(&server)).unwrap();
        let out = client.transcribe(vec![1, 2, 3], Some("en")).await.unwrap();
        assert_eq!(out, "yes");
    }

    #[test]
    fn debug_redacts_api_key() {
        let c = ClientConfig {
            base_url: "http://example.com/v1".to_string(),
            model: "m".to_string(),
            api_key: Some("k1".to_string()),
            connect_timeout_ms: 0,
            total_timeout_ms: 0,
        };
        let s = format!("{c:?}");
        assert!(!s.contains("k1"));
        assert!(s.contains("***"));
    }

    #[tokio::test]
    async fn chat_empty_content_is_bad_response() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/v1/chat/completions");
            then.status(200).json_body(serde_json::json!({
                "choices": [{"message": {"content": ""}}]
            }));
        });
        let client = OpenAiClient::new(cfg(&server)).unwrap();
        let msgs = [ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let err = client.chat(&msgs, 100, 0.5).await.unwrap_err();
        assert!(matches!(err, ProviderError::BadResponse(_)));
    }

    #[tokio::test]
    async fn chat_with_disable_thinking_sends_chat_template_kwargs() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/v1/chat/completions")
                .body_contains("\"enable_thinking\":false");
            then.status(200).json_body(serde_json::json!({
                "choices": [{"message": {"content": "no-think"}}]
            }));
        });
        let client = OpenAiClient::new(cfg(&server)).unwrap();
        let msgs = [ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let out = client.chat_with(&msgs, 100, 0.5, true).await.unwrap();
        assert_eq!(out, "no-think");
    }

    #[tokio::test]
    async fn chat_does_not_send_chat_template_kwargs() {
        let server = MockServer::start_async().await;
        // Only matches the plain body (no chat_template_kwargs key): with sorted
        // JSON keys, the plain body starts with {"max_tokens" while a body
        // containing chat_template_kwargs does not.
        server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/v1/chat/completions")
                .body_contains("{\"max_tokens\"");
            then.status(200).json_body(serde_json::json!({
                "choices": [{"message": {"content": "plain"}}]
            }));
        });
        let client = OpenAiClient::new(cfg(&server)).unwrap();
        let msgs = [ChatMessage {
            role: "user".to_string(),
            content: "hi".to_string(),
        }];
        let out = client.chat(&msgs, 100, 0.5).await.unwrap();
        assert_eq!(out, "plain");
    }

    #[tokio::test]
    async fn transcribe_with_prompt_sends_prompt() {
        let server = MockServer::start_async().await;
        server.mock(|when, then| {
            when.method(httpmock::Method::POST)
                .path("/v1/audio/transcriptions")
                .body_contains("Launch");
            then.status(200)
                .json_body(serde_json::json!({"text": "ok"}));
        });
        let client = OpenAiClient::new(cfg(&server)).unwrap();
        let out = client
            .transcribe_with_prompt(vec![1, 2, 3], Some("zh"), Some("Launch"))
            .await
            .unwrap();
        assert_eq!(out, "ok");
    }
}
