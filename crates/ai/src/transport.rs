//! The transport: what FigCraft asks of a model, and what a model answers.
//!
//! The crate holds no networking. A [`Transport`] is the seam: the desktop build fills it with
//! one that talks to a provider over HTTPS, the browser build with one that talks through the
//! page's own fetch. Everything above the seam — the chat runtime, the tool loop, the settings —
//! is the same on both.
//!
//! A transport is synchronous and streams through a callback. That is not an accident: FigCraft
//! has no async runtime, so a chat runs on a worker thread and reports back through a channel,
//! exactly like the canvas renderer and the control server do.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A tool the model may call.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tool {
    /// The name the model calls it by.
    pub name: String,
    /// What the tool does, in one or two sentences.
    pub description: String,
    /// The tool's parameters, as a JSON Schema object.
    pub input_schema: Value,
}

/// A tool the model asked for.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// The call's id, which the answer must quote.
    pub id: String,
    /// The tool's name.
    pub name: String,
    /// The arguments, as JSON.
    pub arguments: Value,
}

/// Who said something.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    /// The person.
    User,
    /// The model.
    Assistant,
    /// The tools, answering a call.
    Tool,
}

/// One turn of the conversation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatMessage {
    /// Who said it.
    pub role: Role,
    /// What was said.
    pub content: String,
    /// The tool call this message answers, for a tool message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    /// The tool call this message made, for an assistant message.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_calls: Vec<ToolCall>,
}

impl ChatMessage {
    /// A message from the person.
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: Role::User, content: content.into(), tool_call_id: None, tool_calls: Vec::new() }
    }

    /// A message from the model.
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: Role::Assistant, content: content.into(), tool_call_id: None, tool_calls: Vec::new() }
    }

    /// A message from the model that calls tools.
    pub fn assistant_calls(content: impl Into<String>, tool_calls: Vec<ToolCall>) -> Self {
        Self { role: Role::Assistant, content: content.into(), tool_call_id: None, tool_calls }
    }

    /// A tool's answer.
    pub fn tool_result(tool_call_id: impl Into<String>, content: impl Into<String>) -> Self {
        Self { role: Role::Tool, content: content.into(), tool_call_id: Some(tool_call_id.into()), tool_calls: Vec::new() }
    }
}

/// What a model is asked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    /// The model id the provider expects.
    pub model: String,
    /// The endpoint the request goes to (`https://api.openai.com/v1`).
    pub base_url: String,
    /// The provider's key.
    pub api_key: String,
    /// The wire protocol the endpoint speaks.
    pub protocol: crate::providers::Protocol,
    /// Whether an OpenAI-protocol endpoint takes the Responses API instead.
    pub responses_api: bool,
    /// The conversation so far.
    pub messages: Vec<ChatMessage>,
    /// The system instructions.
    pub system: String,
    /// The tools the model may call.
    #[serde(default)]
    pub tools: Vec<Tool>,
    /// The most the model may write.
    #[serde(default)]
    pub max_output_tokens: u32,
    /// The provider's own reasoning setting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<String>,
}

/// A piece of an answer, as it arrives.
#[derive(Clone, Debug, PartialEq)]
pub enum Chunk {
    /// A piece of the answer's text.
    Text(String),
    /// A piece of the model's reasoning.
    Reasoning(String),
    /// A tool the model asked for.
    ToolCall(ToolCall),
}

/// What a model answered.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ChatResponse {
    /// The answer's text.
    pub text: String,
    /// The tools the model asked for.
    pub tool_calls: Vec<ToolCall>,
    /// The tokens the provider reported reading.
    pub input_tokens: Option<u64>,
    /// The tokens the provider reported writing.
    pub output_tokens: Option<u64>,
    /// The tokens the provider reported reading from its cache.
    pub cache_read_tokens: Option<u64>,
    /// The tokens the provider reported writing to its cache.
    pub cache_write_tokens: Option<u64>,
    /// Why the answer ended, when the provider said.
    pub finish_reason: Option<String>,
}

/// Why a request failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportError {
    /// The key is missing, expired or refused.
    Authentication,
    /// The account has no credit left.
    InsufficientCredit,
    /// The model id is not one the provider knows.
    ModelNotFound,
    /// The endpoint speaks another API shape than the one asked for.
    ApiTypeMismatch,
    /// The rate limit was reached.
    RateLimited,
    /// The model ran out of output tokens.
    OutputLimit,
    /// The endpoint could not be reached.
    Network,
    /// The provider answered with something FigCraft could not read.
    BadResponse(String),
    /// The request was stopped on purpose.
    Cancelled,
    /// Anything else.
    Other(String),
}

impl TransportError {
    /// Whether the failure is the person's key being wrong, rather than the request's.
    ///
    /// These read differently in the chat: a bad key is fixed in the settings, a network failure
    /// is retried.
    pub fn is_credential_problem(&self) -> bool {
        matches!(self, Self::Authentication | Self::InsufficientCredit | Self::ModelNotFound | Self::ApiTypeMismatch)
    }

    /// A short sentence for the chat to show.
    pub fn message(&self) -> String {
        match self {
            Self::Authentication => "The provider refused the API key. Replace it in Settings.".into(),
            Self::InsufficientCredit => "The provider account has insufficient credit or quota.".into(),
            Self::ModelNotFound => "The selected model is unavailable. Choose another model in Settings.".into(),
            Self::ApiTypeMismatch => "The endpoint does not support the selected API type. Try Completions or Responses.".into(),
            Self::RateLimited => "The provider rate limit was reached. Wait a moment and try again.".into(),
            Self::OutputLimit => "The model ran out of output tokens while reasoning.".into(),
            Self::Network => "Could not reach the provider. Check the connection and the endpoint.".into(),
            Self::BadResponse(why) => format!("The provider's answer could not be read: {why}"),
            Self::Cancelled => "The request was stopped.".into(),
            Self::Other(why) => why.clone(),
        }
    }
}

/// One piece of a request's progress, as a build without threads reports it.
///
/// [`Transport::complete`] streams through a borrowed callback, which a page cannot do: its
/// network call is a promise and the app's frame loop shares the one thread. A build like that
/// reports the same progress through [`TransportFactory::start`] with this instead.
#[derive(Clone, Debug)]
pub enum StreamEvent {
    /// A piece of the answer's text.
    Text(String),
    /// A piece of the model's reasoning.
    Reasoning(String),
    /// The request is over, with the whole answer or the reason it failed.
    Done(Result<ChatResponse, TransportError>),
}

/// A model's answer, streamed.
pub trait Transport: Send {
    /// Ask a model, reporting each piece as it arrives.
    ///
    /// The callback is called on the calling thread, so an implementation that streams must do
    /// its reading in the same call.
    fn complete(&self, request: &ChatRequest, on_chunk: &mut dyn FnMut(Chunk)) -> Result<ChatResponse, TransportError>;

    /// Whether the transport can reach anything at all.
    ///
    /// A transport with no key, or one the platform does not offer, says so here rather than
    /// failing on the first message.
    fn available(&self) -> bool {
        true
    }
}

/// The factory a worker thread makes its transport from.
///
/// A transport runs on one thread at a time; a fresh one per request keeps that obvious. The
/// app installs the factory the platform offers; the default makes transports that say the
/// platform has none.
pub trait TransportFactory: Send + Sync {
    /// A transport for one request. The request is passed so the factory can choose ACP for an
    /// external agent and HTTPS for a model provider.
    fn make(&self, request: &ChatRequest) -> Box<dyn Transport>;

    /// Run one request without a worker thread, reporting through `on_event`, and finish the
    /// work later — a build with no threads runs it on the event loop and returns.
    ///
    /// Most builds answer `false` and let the caller spawn a thread and call
    /// [`Transport::complete`], which is what the desktop app does. A page cannot spawn a thread,
    /// so its factory answers `true`, starts the request as a promise and reports into
    /// `on_event` as it arrives. `on_event` is called on the thread that drives the request.
    fn start(&self, _request: ChatRequest, _on_event: Box<dyn FnMut(StreamEvent) + Send>) -> bool {
        false
    }
}

/// The factory with nothing behind it.
#[derive(Default)]
pub struct NoTransport;

impl TransportFactory for NoTransport {
    fn make(&self, _request: &ChatRequest) -> Box<dyn Transport> {
        Box::new(NoTransportTransport)
    }
}

/// The transport the default factory makes: there is nothing here.
#[derive(Default)]
struct NoTransportTransport;

impl Transport for NoTransportTransport {
    fn available(&self) -> bool {
        false
    }

    fn complete(&self, _request: &ChatRequest, _on_chunk: &mut dyn FnMut(Chunk)) -> Result<ChatResponse, TransportError> {
        Err(TransportError::Other("this build has no provider connection".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_knows_who_said_it() {
        assert_eq!(ChatMessage::user("hi").role, Role::User);
        assert_eq!(ChatMessage::assistant("hello").role, Role::Assistant);
        let t = ChatMessage::tool_result("c1", "done");
        assert_eq!(t.role, Role::Tool);
        assert_eq!(t.tool_call_id.as_deref(), Some("c1"));
    }

    #[test]
    fn a_credential_failure_reads_differently_from_a_network_one() {
        assert!(TransportError::Authentication.is_credential_problem());
        assert!(!TransportError::Network.is_credential_problem());
        assert!(TransportError::Authentication.message().contains("API key"));
    }
}
