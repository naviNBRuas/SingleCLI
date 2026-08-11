pub mod config;
pub mod lsp;
pub mod mcp;
pub mod paths;
pub mod permissions;
pub mod profile;
pub mod registry;
pub mod secrets;
pub mod skills;
pub mod tools;

pub use config::{merge_layers, resolve, ConfigLayer, ResolvedConfig};
pub use paths::SingleDirs;
pub use registry::{builtin_registry, AgentDefinition};
