//! The curated plugin directory, as data.
//!
//! FigCraft's plugins are real WASM modules with a manifest and a metered sandbox; the directory
//! is the curated list a person browses. An entry says plainly whether it ships a runnable module
//! or is catalog-only, so the UI never offers a Run button on something that cannot run.

use serde::{Deserialize, Serialize};

/// A directory category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PluginCategory {
    Accessibility,
    DesignSystem,
    Layout,
    Assets,
    Utility,
}

impl PluginCategory {
    /// The category's label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Accessibility => "Accessibility",
            Self::DesignSystem => "Design system",
            Self::Layout => "Layout",
            Self::Assets => "Assets",
            Self::Utility => "Utility",
        }
    }
}

/// Whether an entry can actually be run.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginRuntime {
    /// Ships a module; the field names the plugin's command.
    Wasm { module: String },
    /// A catalog entry without a published artifact. The reason is shown.
    Unavailable { reason: String },
}

/// One directory entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DirectoryPlugin {
    /// The plugin's id.
    pub id: String,
    /// The name shown.
    pub name: String,
    /// The version.
    pub version: String,
    /// The author.
    pub author: String,
    /// What it does.
    pub description: String,
    /// Its category.
    pub category: PluginCategory,
    /// The permissions it asks for.
    pub permissions: Vec<String>,
    /// Whether it can run here.
    pub runtime: PluginRuntime,
}

/// The catalog, in the order it is shown.
///
/// The example plugins ship with FigCraft's plugin workspace; the others are catalog-only until a
/// build is published. Nothing here claims a third-party icon or a licensed asset — the entries
/// are names and descriptions only.
pub fn catalog() -> &'static [DirectoryPlugin] {
    &CATALOG
}

static CATALOG: std::sync::LazyLock<Vec<DirectoryPlugin>> = std::sync::LazyLock::new(|| {
    vec![
        DirectoryPlugin {
            id: "tidy-names".into(),
            name: "Tidy names".into(),
            version: "1.0.0".into(),
            author: "FigCraft".into(),
            description: "Give selected layers consistent, readable names.".into(),
            category: PluginCategory::Utility,
            permissions: vec!["selection:read".into(), "node:write".into()],
            runtime: PluginRuntime::Wasm { module: "tidy_names".into() },
        },
        DirectoryPlugin {
            id: "swatch-grid".into(),
            name: "Swatch grid".into(),
            version: "1.0.0".into(),
            author: "FigCraft".into(),
            description: "Lay out the document's colours as a labelled swatch grid.".into(),
            category: PluginCategory::DesignSystem,
            permissions: vec!["variables:read".into(), "node:write".into()],
            runtime: PluginRuntime::Wasm { module: "swatch_grid".into() },
        },
        DirectoryPlugin {
            id: "export-names".into(),
            name: "Export names".into(),
            version: "1.0.0".into(),
            author: "FigCraft".into(),
            description: "Export the selected layers' names as a table.".into(),
            category: PluginCategory::Assets,
            permissions: vec!["selection:read".into()],
            runtime: PluginRuntime::Wasm { module: "export_names".into() },
        },
        DirectoryPlugin {
            id: "wcag-contrast".into(),
            name: "Contrast checker".into(),
            version: "1.0.0".into(),
            author: "FigCraft".into(),
            description: "Check WCAG contrast of text against its background.".into(),
            category: PluginCategory::Accessibility,
            permissions: vec!["selection:read".into(), "color:read".into()],
            runtime: PluginRuntime::Unavailable { reason: "No published build yet.".into() },
        },
        DirectoryPlugin {
            id: "padding-linter".into(),
            name: "Padding linter".into(),
            version: "1.0.0".into(),
            author: "FigCraft".into(),
            description: "Find padding and spacing off the 4px grid.".into(),
            category: PluginCategory::Layout,
            permissions: vec!["selection:read".into()],
            runtime: PluginRuntime::Unavailable { reason: "No published build yet.".into() },
        },
    ]
});

/// The result of one plugin run, for the panel's "Last run" card.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PluginRunSummary {
    /// The plugin that ran.
    pub plugin: String,
    /// What it set as its output, when it did.
    pub output: Option<String>,
    /// Its log lines.
    pub logs: Vec<String>,
    /// How many host calls it made.
    pub calls: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runnable_entries_name_a_module() {
        for entry in catalog() {
            match &entry.runtime {
                PluginRuntime::Wasm { module } => assert!(!module.is_empty(), "{} names a module", entry.id),
                PluginRuntime::Unavailable { reason } => assert!(!reason.is_empty(), "{} explains itself", entry.id),
            }
        }
    }

    #[test]
    fn ids_are_unique() {
        for (i, entry) in catalog().iter().enumerate() {
            assert!(!catalog()[..i].iter().any(|e| e.id == entry.id), "{} is unique", entry.id);
        }
    }
}
