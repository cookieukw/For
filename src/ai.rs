//! User-triggered drafts from a local or explicitly selected hosted model.
//!
//! A request contains the latest incoming message and the amount of archived
//! context the reader configured. Local endpoints are restricted to loopback.
//! ZapFast never sends a generated reply automatically.

use std::fmt;
use std::time::Duration;

use reqwest::Url;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

/// Model providers available in AI settings. Hosted providers are used only
/// when the reader explicitly requests a draft or synthetic simulator test.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiProvider {
    #[default]
    Local,
    OpenAi,
    DeepSeek,
    Anthropic,
    Gemini,
    OpenRouter,
}

impl AiProvider {
    pub const ALL: [Self; 6] = [
        Self::Local,
        Self::OpenAi,
        Self::DeepSeek,
        Self::Anthropic,
        Self::Gemini,
        Self::OpenRouter,
    ];

    pub fn keyring_id(self) -> &'static str {
        match self {
            Self::Local => "local-model",
            Self::OpenAi => "openai",
            Self::DeepSeek => "deepseek",
            Self::Anthropic => "anthropic",
            Self::Gemini => "gemini",
            Self::OpenRouter => "openrouter",
        }
    }

    pub fn label(self, locale: crate::i18n::Locale) -> String {
        match self {
            Self::Local => crate::i18n::gettext(locale, "Local (Ollama / LM Studio)").into_owned(),
            Self::OpenAi => crate::i18n::gettext(locale, "OpenAI (ChatGPT)").into_owned(),
            Self::DeepSeek => crate::i18n::gettext(locale, "DeepSeek").into_owned(),
            Self::Anthropic => crate::i18n::gettext(locale, "Anthropic (Claude)").into_owned(),
            Self::Gemini => crate::i18n::gettext(locale, "Google (Gemini)").into_owned(),
            Self::OpenRouter => crate::i18n::gettext(locale, "OpenRouter").into_owned(),
        }
    }

    pub fn is_local(self) -> bool {
        self == Self::Local
    }
}

/// Secret API key passed to the worker for OS-keyring storage.
pub struct ApiKey(Zeroizing<String>);

impl ApiKey {
    pub fn new(value: String) -> Self {
        Self(Zeroizing::new(value))
    }

    pub fn expose(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Debug for ApiKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApiKey([REDACTED])")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptRole {
    User,
    Assistant,
}

/// One turn of the selected conversation. Its text is redacted from diagnostics.
pub struct PromptMessage {
    pub role: PromptRole,
    pub content: String,
}

impl PromptMessage {
    pub fn new(role: PromptRole, content: String) -> Self {
        Self { role, content }
    }
}

impl fmt::Debug for PromptMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PromptMessage")
            .field("role", &self.role)
            .field("content", &"[REDACTED]")
            .finish()
    }
}

/// Explicitly selected conversation context. Message bodies never appear in
/// command diagnostics.
pub struct Prompt(Vec<PromptMessage>);

impl Prompt {
    pub fn new(value: String) -> Self {
        Self(vec![PromptMessage::new(PromptRole::User, value)])
    }

    pub fn with_history(mut self, mut history: Vec<PromptMessage>) -> Self {
        history.append(&mut self.0);
        Self(history)
    }

    pub fn messages(&self) -> &[PromptMessage] {
        &self.0
    }
}

impl fmt::Debug for Prompt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Prompt({} messages, [REDACTED])", self.0.len())
    }
}

/// One user-triggered draft. `context_messages` is the maximum number of
/// archived messages including the selected latest incoming message; `None`
/// means all locally archived messages up to it.
pub struct DraftRequest {
    pub request: u64,
    pub chat: Option<crate::model::ChatId>,
    pub provider: AiProvider,
    pub endpoint: String,
    pub model: String,
    pub style: String,
    pub message: Prompt,
    pub message_id: Option<String>,
    pub context_messages: Option<usize>,
}

impl fmt::Debug for DraftRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DraftRequest")
            .field("request", &self.request)
            .field("provider", &self.provider)
            .field("model", &self.model)
            .field("has_chat", &self.chat.is_some())
            .field("has_message_id", &self.message_id.is_some())
            .field("context_messages", &self.context_messages)
            .field("message", &self.message)
            .field("endpoint", &"[REDACTED]")
            .field("style", &"[REDACTED]")
            .finish()
    }
}

/// Generated reply/error crossing from the worker to the UI. Successful
/// reply text is personal content and is redacted from `Debug`.
pub struct DraftResult(pub Result<String, String>);

impl fmt::Debug for DraftResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Ok(_) => formatter.write_str("DraftResult(Ok([REDACTED]))"),
            Err(error) => formatter.debug_tuple("DraftResult").field(error).finish(),
        }
    }
}

/// Normalizes the configured server and refuses any non-loopback host.
pub fn local_base(endpoint: &str) -> Result<String, String> {
    let mut url =
        Url::parse(endpoint.trim()).map_err(|_| "Enter a valid local model URL".to_owned())?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("The local model URL must use HTTP or HTTPS".to_owned());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "The local model URL has no host".to_owned())?;
    let local = host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback());
    if !local {
        return Err("A local model URL must point to localhost or a loopback IP".to_owned());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Put the API key in AI settings, not in the URL".to_owned());
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

fn client(provider: AiProvider) -> Result<Client, String> {
    let mut builder = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(90));
    if provider.is_local() {
        builder = builder.no_proxy();
    }
    builder
        .build()
        .map_err(|error| format!("Could not start the AI client: {error}"))
}

fn api_key(provider: AiProvider) -> Result<Option<Zeroizing<String>>, String> {
    crate::archive::ai_api_key(provider.keyring_id()).map_err(|error| error.to_string())
}

fn base_url(provider: AiProvider, local_endpoint: &str) -> Result<String, String> {
    match provider {
        AiProvider::Local => local_base(local_endpoint),
        AiProvider::OpenAi => Ok("https://api.openai.com/v1".into()),
        AiProvider::DeepSeek => Ok("https://api.deepseek.com".into()),
        AiProvider::Anthropic => Ok("https://api.anthropic.com/v1".into()),
        AiProvider::Gemini => Ok("https://generativelanguage.googleapis.com/v1beta".into()),
        AiProvider::OpenRouter => Ok("https://openrouter.ai/api/v1".into()),
    }
}

fn key_for_request(provider: AiProvider) -> Result<Option<Zeroizing<String>>, String> {
    let key = api_key(provider)?;
    if !provider.is_local() && key.as_ref().is_none_or(|key| key.trim().is_empty()) {
        return Err(format!(
            "Save an API key for {} in AI settings",
            provider.label(crate::i18n::Locale::English)
        ));
    }
    Ok(key)
}

fn authorize(
    request: reqwest::blocking::RequestBuilder,
    provider: AiProvider,
    key: Option<&str>,
) -> reqwest::blocking::RequestBuilder {
    match (provider, key) {
        (_, None) => request,
        (AiProvider::Anthropic, Some(key)) => request
            .header("x-api-key", key)
            .header("anthropic-version", "2023-06-01"),
        (AiProvider::Gemini, Some(key)) => request.header("x-goog-api-key", key),
        (_, Some(key)) => request.bearer_auth(key),
    }
}

fn parse_models(provider: AiProvider, body: &serde_json::Value) -> Vec<String> {
    let mut models: Vec<String> = match provider {
        AiProvider::Gemini => body
            .get("models")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter(|model| {
                model
                    .get("supportedGenerationMethods")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|methods| {
                        methods
                            .iter()
                            .any(|method| method.as_str() == Some("generateContent"))
                    })
            })
            .filter_map(|model| model.get("name").and_then(serde_json::Value::as_str))
            .map(|name| name.strip_prefix("models/").unwrap_or(name).to_owned())
            .collect(),
        _ => body
            .get("data")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|model| model.get("id").and_then(serde_json::Value::as_str))
            .filter(|model| !model.is_empty())
            .map(str::to_owned)
            .collect(),
    };
    models.sort();
    models.dedup();
    models
}

fn openai_compatible_body(model: &str, system: &str, prompt: &Prompt) -> serde_json::Value {
    let messages = std::iter::once(serde_json::json!({
        "role": "system",
        "content": system,
    }))
    .chain(prompt.messages().iter().map(|message| {
        let role = match message.role {
            PromptRole::User => "user",
            PromptRole::Assistant => "assistant",
        };
        serde_json::json!({ "role": role, "content": message.content })
    }))
    .collect::<Vec<_>>();
    serde_json::json!({
        "model": model,
        "messages": messages,
        "stream": false,
    })
}

fn response_text(value: &serde_json::Value) -> Option<String> {
    let parts: Vec<&str> = match value {
        serde_json::Value::String(text) => vec![text],
        serde_json::Value::Array(values) => values
            .iter()
            .filter_map(|value| {
                value
                    .as_str()
                    .or_else(|| value.get("text").and_then(serde_json::Value::as_str))
            })
            .collect(),
        serde_json::Value::Object(_) => value
            .get("text")
            .and_then(serde_json::Value::as_str)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    };
    let text = parts
        .into_iter()
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    (!text.is_empty()).then_some(text)
}

fn openrouter_empty_reply_error(body: &serde_json::Value) -> String {
    if body.get("error").is_some() {
        return "OpenRouter returned an API error instead of a reply. Check the selected model and your OpenRouter account limits.".into();
    }
    let choice = body.pointer("/choices/0");
    if choice
        .and_then(|choice| choice.pointer("/message/tool_calls"))
        .and_then(serde_json::Value::as_array)
        .is_some_and(|calls| !calls.is_empty())
    {
        return "OpenRouter selected a model that returned tool calls instead of reply text. Choose a text chat model.".into();
    }
    match choice
        .and_then(|choice| choice.get("finish_reason"))
        .and_then(serde_json::Value::as_str)
    {
        Some("length") => {
            "OpenRouter ended the response before producing reply text. Try a different model.".into()
        }
        Some("content_filter") => {
            "OpenRouter filtered the response and returned no reply text.".into()
        }
        _ => "OpenRouter returned no reply text. Try a text chat model or another model from its list.".into(),
    }
}

fn system_instruction(style: &str) -> String {
    format!(
        "In this conversation context, `user` messages are written by the contact and `assistant` messages (called `model` by Gemini) are written by the WhatsApp account owner. The final `user` message is the contact's message to answer. Interpret first- and second-person words from the author of each turn; never swap the speakers. Write from the account owner's perspective and address the contact. Return only the reply text. The user will review and send it manually. Do not claim to have sent anything. Style: {style}"
    )
}

/// Lists current models from the selected provider's model endpoint.
pub fn list_models(provider: AiProvider, endpoint: &str) -> Result<Vec<String>, String> {
    let base = base_url(provider, endpoint)?;
    let url = match provider {
        AiProvider::Gemini => format!("{base}/models"),
        AiProvider::Anthropic => format!("{base}/models"),
        _ => format!("{base}/models"),
    };
    let key = key_for_request(provider)?;
    let request = authorize(
        client(provider)?.get(url),
        provider,
        key.as_deref().map(String::as_str),
    );
    let response = request
        .send()
        .map_err(|error| {
            format!(
                "Could not reach the {} model service: {error}",
                provider.label(crate::i18n::Locale::English)
            )
        })?
        .error_for_status()
        .map_err(|error| {
            format!(
                "The {} model service rejected the request: {error}",
                provider.label(crate::i18n::Locale::English)
            )
        })?;
    let body = response.text().map_err(|error| {
        format!(
            "The {} model service returned invalid model data: {error}",
            provider.label(crate::i18n::Locale::English)
        )
    })?;
    let body: serde_json::Value = serde_json::from_str(&body).map_err(|error| {
        format!(
            "The {} model service returned invalid model data: {error}",
            provider.label(crate::i18n::Locale::English)
        )
    })?;
    Ok(parse_models(provider, &body))
}

/// Generates a draft from the selected message and explicitly configured
/// context. This interface has no access to chats or the archive itself.
pub fn draft(
    provider: AiProvider,
    endpoint: &str,
    model: &str,
    style: &str,
    prompt: &Prompt,
) -> Result<String, String> {
    let base = base_url(provider, endpoint)?;
    if model.trim().is_empty() {
        return Err("Choose a model in AI settings first".to_owned());
    }
    if prompt.messages().is_empty()
        || prompt
            .messages()
            .last()
            .is_none_or(|message| message.content.trim().is_empty())
    {
        return Err("The latest message has no text to answer".to_owned());
    }
    let style = style.trim();
    let style_instruction = if style.is_empty() {
        "Use a natural, concise tone and reply in the same language as the message. Do not invent facts."
    } else {
        style
    };
    let system = system_instruction(style_instruction);
    let key = key_for_request(provider)?;
    let (url, request_body) = match provider {
        AiProvider::Local | AiProvider::OpenAi | AiProvider::DeepSeek | AiProvider::OpenRouter => (
            format!("{base}/chat/completions"),
            openai_compatible_body(model, &system, prompt),
        ),
        AiProvider::Anthropic => (
            format!("{base}/messages"),
            serde_json::json!({
                "model": model,
                "max_tokens": 512,
                "system": system,
                "messages": prompt.messages().iter().map(|message| {
                    let role = match message.role {
                        PromptRole::User => "user",
                        PromptRole::Assistant => "assistant",
                    };
                    serde_json::json!({ "role": role, "content": message.content })
                }).collect::<Vec<_>>(),
            }),
        ),
        AiProvider::Gemini => {
            let model = model.strip_prefix("models/").unwrap_or(model);
            (
                format!("{base}/models/{model}:generateContent"),
                serde_json::json!({
                    "systemInstruction": { "parts": [{ "text": system }] },
                    "contents": prompt.messages().iter().map(|message| {
                        let role = match message.role {
                            PromptRole::User => "user",
                            PromptRole::Assistant => "model",
                        };
                        serde_json::json!({
                            "role": role,
                            "parts": [{ "text": message.content }],
                        })
                    }).collect::<Vec<_>>(),
                    "generationConfig": { "maxOutputTokens": 512 },
                }),
            )
        }
    };
    let request = authorize(
        client(provider)?.post(url),
        provider,
        key.as_deref().map(String::as_str),
    );
    let response = request
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(request_body.to_string())
        .send()
        .map_err(|error| {
            format!(
                "Could not reach the {} model service: {error}",
                provider.label(crate::i18n::Locale::English)
            )
        })?
        .error_for_status()
        .map_err(|error| {
            format!(
                "The {} model service rejected the request: {error}",
                provider.label(crate::i18n::Locale::English)
            )
        })?;
    let body = response.text().map_err(|error| {
        format!(
            "The {} model service returned an invalid reply: {error}",
            provider.label(crate::i18n::Locale::English)
        )
    })?;
    let body: serde_json::Value = serde_json::from_str(&body).map_err(|error| {
        format!(
            "The {} model service returned an invalid reply: {error}",
            provider.label(crate::i18n::Locale::English)
        )
    })?;
    if provider == AiProvider::OpenRouter && body.get("error").is_some() {
        return Err(openrouter_empty_reply_error(&body));
    }
    let text = match provider {
        AiProvider::Local | AiProvider::OpenAi | AiProvider::DeepSeek | AiProvider::OpenRouter => {
            body.pointer("/choices/0/message/content")
                .and_then(response_text)
                .or_else(|| {
                    body.pointer("/choices/0/message/refusal")
                        .and_then(response_text)
                })
        }
        AiProvider::Anthropic => body.get("content").and_then(response_text),
        AiProvider::Gemini => body
            .pointer("/candidates/0/content/parts")
            .and_then(response_text),
    };
    text.ok_or_else(|| match provider {
        AiProvider::OpenRouter => openrouter_empty_reply_error(&body),
        _ => "The model returned no reply text".to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::{
        AiProvider, Prompt, PromptMessage, PromptRole, base_url, local_base,
        openai_compatible_body, openrouter_empty_reply_error, parse_models, response_text,
        system_instruction,
    };

    #[test]
    fn the_model_endpoint_must_be_on_loopback() {
        assert_eq!(
            local_base("http://localhost:11434/v1/").unwrap(),
            "http://localhost:11434/v1"
        );
        assert_eq!(
            local_base("http://127.0.0.1:1234/v1").unwrap(),
            "http://127.0.0.1:1234/v1"
        );
        assert!(local_base("https://api.openai.com/v1").is_err());
        assert!(local_base("http://192.168.1.10:11434/v1").is_err());
        assert_eq!(AiProvider::ALL.len(), 6);
        assert_eq!(
            base_url(AiProvider::OpenRouter, "").unwrap(),
            "https://openrouter.ai/api/v1"
        );
        assert_eq!(AiProvider::OpenRouter.keyring_id(), "openrouter");
    }

    #[test]
    fn model_catalogs_follow_each_provider_response() {
        let openai = serde_json::json!({
            "data": [{"id":"gpt-live"}, {"id":"gpt-live"}, {"id":"gpt-next"}]
        });
        assert_eq!(
            parse_models(AiProvider::OpenAi, &openai),
            ["gpt-live", "gpt-next"]
        );
        let gemini = serde_json::json!({
            "models": [
                {"name":"models/gemini-chat", "supportedGenerationMethods":["generateContent"]},
                {"name":"models/gemini-embedding", "supportedGenerationMethods":["embedContent"]}
            ]
        });
        assert_eq!(parse_models(AiProvider::Gemini, &gemini), ["gemini-chat"]);
        let openrouter = serde_json::json!({
            "data": [{"id":"anthropic/claude-model"}, {"id":"openai/gpt-model"}]
        });
        assert_eq!(
            parse_models(AiProvider::OpenRouter, &openrouter),
            ["anthropic/claude-model", "openai/gpt-model"]
        );
    }

    #[test]
    fn prompt_keeps_roles_and_redacts_every_message_body() {
        let prompt = Prompt::new("new incoming text".into()).with_history(vec![
            PromptMessage::new(PromptRole::Assistant, "old outgoing text".into()),
            PromptMessage::new(PromptRole::User, "old incoming text".into()),
        ]);
        assert_eq!(prompt.messages().len(), 3);
        assert_eq!(prompt.messages()[0].role, PromptRole::Assistant);
        assert_eq!(prompt.messages()[2].content, "new incoming text");
        let diagnostic = format!("{prompt:?}");
        assert!(!diagnostic.contains("outgoing text"));
        assert!(!diagnostic.contains("incoming text"));
    }

    #[test]
    fn draft_request_debug_does_not_expose_chat_or_prompt_data() {
        let request = super::DraftRequest {
            request: 1,
            chat: Some("15550001111@s.whatsapp.net".into()),
            provider: AiProvider::OpenRouter,
            endpoint: String::new(),
            model: "provider/model".into(),
            style: "private style".into(),
            message: Prompt::new("private message".into()),
            message_id: Some("message-id".into()),
            context_messages: Some(3),
        };
        let diagnostic = format!("{request:?}");
        assert!(!diagnostic.contains("15550001111"));
        assert!(!diagnostic.contains("private style"));
        assert!(!diagnostic.contains("private message"));
        assert!(!diagnostic.contains("message-id"));
    }

    #[test]
    fn openai_compatible_request_preserves_context_turn_order() {
        let prompt = Prompt::new("latest incoming".into()).with_history(vec![
            PromptMessage::new(PromptRole::User, "older incoming".into()),
            PromptMessage::new(PromptRole::Assistant, "our reply".into()),
        ]);
        let body = openai_compatible_body("provider/model", "system", &prompt);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[1]["content"], "older incoming");
        assert_eq!(messages[2]["role"], "assistant");
        assert_eq!(messages[2]["content"], "our reply");
        assert_eq!(messages[3]["content"], "latest incoming");
    }

    #[test]
    fn openrouter_response_parser_accepts_text_blocks() {
        let blocks = serde_json::json!([
            {"type":"text", "text":"Suggested "},
            {"type":"text", "text":"reply"}
        ]);
        assert_eq!(response_text(&blocks).as_deref(), Some("Suggested\nreply"));
        assert_eq!(
            response_text(&serde_json::json!(" plain text ")).as_deref(),
            Some("plain text")
        );
        assert_eq!(
            response_text(&serde_json::json!({"type":"text", "text":"object text"})).as_deref(),
            Some("object text")
        );
    }

    #[test]
    fn openrouter_empty_response_explains_non_text_finish_reasons() {
        let tool_call = serde_json::json!({
            "choices": [{"finish_reason":"tool_calls", "message":{"tool_calls":[{}]}}]
        });
        assert!(openrouter_empty_reply_error(&tool_call).contains("tool calls"));
        let truncated = serde_json::json!({
            "choices": [{"finish_reason":"length", "message":{"content":null}}]
        });
        assert!(openrouter_empty_reply_error(&truncated).contains("before producing reply text"));
        let api_error =
            serde_json::json!({"error":{"code":402,"message":"private provider detail"}});
        let error = openrouter_empty_reply_error(&api_error);
        assert!(error.contains("API error"));
        assert!(!error.contains("private provider detail"));
    }

    #[test]
    fn system_instruction_spells_out_who_each_chat_role_represents() {
        let instruction = system_instruction("Warm and concise");
        assert!(instruction.contains("`user` messages are written by the contact"));
        assert!(instruction.contains("`assistant` messages (called `model` by Gemini)"));
        assert!(instruction.contains("never swap the speakers"));
        assert!(instruction.ends_with("Style: Warm and concise"));
    }
}
