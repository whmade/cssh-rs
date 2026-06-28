//! Plugin manifest schema shared between the host and the SDK.
//!
//! Defines the data types backing each plugin's `cssh-rs-plugin.toml`
//! manifest. M0 scope is the manifest schema only; the RPC message
//! catalog, capability negotiation, and version-matching logic land in
//! M2 alongside the NDJSON codec.

#![deny(clippy::implicit_return)]
#![allow(clippy::needless_return, clippy::doc_overindented_list_items)]
#![warn(missing_docs)]
#![doc(html_no_source)]

use serde_derive::{Deserialize, Serialize};

/// Kind of plugin a manifest describes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// Opens a terminal window per host and launches the cssh-rs client
    /// inside it.
    Spawner,
    /// Places, raises, and retitles client windows after spawn.
    WindowManager,
}

/// Operating system a plugin can declare support for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Platform {
    /// Microsoft Windows.
    Windows,
    /// Linux.
    Linux,
    /// Apple macOS.
    Macos,
}

/// Parsed `cssh-rs-plugin.toml` manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin identity and protocol metadata.
    pub plugin: PluginMeta,
    /// Capability flags the plugin advertises.
    #[serde(default)]
    pub capabilities: Capabilities,
    /// Operating systems the plugin supports.
    pub platforms: Platforms,
    /// How to launch the plugin executable.
    pub runtime: Runtime,
    /// Window-manager-to-spawner matching constraints.
    #[serde(default)]
    pub matching: Matching,
}

/// Plugin identity and protocol metadata (the `[plugin]` section).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginMeta {
    /// Unique plugin name; user plugins shadow bundled ones by name.
    pub name: String,
    /// Plugin version string.
    pub version: String,
    /// Whether this is a spawner or a window manager.
    pub kind: PluginKind,
    /// Wire-protocol version the plugin speaks (e.g. `"1.0"`).
    pub protocol_version: String,
    /// Human-readable description shown to operators.
    pub description: String,
}

/// Capability flags a plugin advertises (the `[capabilities]` section).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Capabilities {
    /// Spawner: accepts an `app_id` window-identity hint.
    pub supports_app_id: bool,
    /// Spawner: sets the initial window title.
    pub supports_title: bool,
    /// Spawner: honours a position/size hint at spawn time.
    pub supports_geometry_hint: bool,
    /// Window manager: can move and resize windows.
    pub supports_place: bool,
    /// Window manager: can bring a window to the front.
    pub supports_raise: bool,
    /// Window manager: can retitle a window after spawn.
    pub supports_set_title: bool,
    /// Window manager: honours an xdg-activation token.
    pub supports_activation_token: bool,
}

/// Operating systems a plugin supports (the `[platforms]` section).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Platforms {
    /// Operating systems the plugin runs on.
    pub supported: Vec<Platform>,
}

/// How to launch the plugin executable (the `[runtime]` section).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Runtime {
    /// Executable name or path relative to the plugin directory.
    pub executable: String,
    /// Extra arguments passed to the executable.
    #[serde(default)]
    pub args: Vec<String>,
}

/// Window-manager-to-spawner matching constraints (the `[matching]`
/// section).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Matching {
    /// Spawner names this window manager can manage; empty means any.
    pub handles_spawners: Vec<String>,
}
