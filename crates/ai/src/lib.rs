//! FigCraft's AI core: the provider registry, model connections and profiles, role assignments,
//! the design-skill registry and router, usage accounting and the transport traits.
//!
//! The crate holds no networking and no interface: a provider is data, a model profile is data,
//! a skill is data. What talks to a provider is a [`transport`] implementation (the desktop build
//! adds one), and what shows the settings is the interface crate, reading the same types.
//!
//! Every type here is serialisable, because the settings live in the engine's preferences and
//! travel to the control channel and the MCP server like every other piece of state.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod acp;
pub mod assets;
pub mod chat_prefs;
pub mod cloud;
pub mod credentials;
pub mod diagnostics;
pub mod history;
pub mod mcp;
pub mod media;
pub mod models;
pub mod plugin_catalog;
pub mod providers;
pub mod skills;
pub mod storage;
pub mod sync;
pub mod transport;
pub mod usage;
pub mod vectorize;
pub mod vision;

pub use acp::{ACPAgentDef, PROTOCOL_VERSION};
pub use assets::{
    AssetKind, AssetSearchOptions, AssetSearchOutcome, ImageAsset, LicensePolicy, MAX_ASSET_BYTES, MAX_TOTAL_CANDIDATES, MAX_VISION_CALLS,
    OPENVERSE_ENDPOINT, USER_AGENT, WIKIMEDIA_ENDPOINT, license_allows_commercial_use, license_allows_modification, license_requires_attribution,
    rank_candidates, validate_remote_url,
};
pub use chat_prefs::ReasoningDisplay;
pub use cloud::{CloudDocument, CloudFactory, CloudSession, NoCloud};
pub use credentials::{CredentialRef, CredentialStatus, CredentialStore, CredentialStoreError};
pub use diagnostics::{DiagnosticCategory, DiagnosticEvent, DiagnosticLevel, DiagnosticLog};
pub use history::{ChatConversation, ChatHistory};
pub use mcp::{MCPAuthentication, MCPConnection, MCPSettings, McpFactory, McpSession, NoMcp};
pub use media::{BoxFuture, MediaFactory, MediaSession, NoMedia, Orientation, StockPhoto, VisionConfig};
pub use models::{AIModelAssignments, AIModelCapability, AIModelConnection, AIModelProfile, AIModelRole, AIModelSettings, ResolvedAIModelRole};
pub use plugin_catalog::{DirectoryPlugin, PluginCategory, PluginRunSummary, PluginRuntime};
pub use providers::{AIProviderDef, Capability, ModelOption, ProviderId};
pub use skills::{PromptSkill, PromptSkillStore, Skill, SkillCommand, SkillRoute, SkillRouter};
pub use storage::{NoStorage, S3Settings, StorageDocument, StorageFactory, StorageProvider, StorageSession};
pub use sync::{SyncJob, SyncKind, SyncOutbox, SyncState};
pub use transport::{ChatMessage, ChatRequest, ChatResponse, NoTransport, StreamEvent, Transport, TransportError, TransportFactory};
pub use usage::{UsageModelSummary, UsageSummary};
pub use vectorize::VectorizeProvider;
pub use vision::{
    ImageRoute, SourcingResult, SourcingState, VisionVerdict, parse_requests as parse_image_requests, parse_verdict as parse_vision_verdict,
};
