pub mod config;
pub mod mcp;
pub mod paths;
pub mod profile;
pub mod registry;

pub use config::{merge_layers, resolve, ConfigLayer, ResolvedConfig};
pub use paths::SingleDirs;
pub use registry::{builtin_registry, AgentDefinition};
