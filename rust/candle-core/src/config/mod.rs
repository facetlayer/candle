//! Config subsystem: `.candle.json` parsing, discovery, validation, and the
//! mutating commands (setup-project / add-service / remove-service / set-config).
//!
//! Ported from `src/configFile.ts`, `src/addServerConfig.ts`,
//! `src/removeServerConfig.ts`, `src/set-config-command.ts`, and
//! `src/setup-project-command.ts`. See `rust/docs/porting/map-config.md`.

pub mod commands;
pub mod file;
pub mod model;
pub mod paths;
pub mod validate;

#[cfg(test)]
pub(crate) mod test_support;

// Re-export the commonly used surface for CLI consumers.
pub use commands::{
    add_server_config, handle_set_config, handle_setup_project, remove_server_config,
    AddServerConfigArgs,
};
pub use file::{
    find_config_file, find_loose_command_name, find_project_dir, find_service_by_name,
    get_all_service_names, get_log_eviction_config, get_service_config_by_name,
    read_config_file, resolve_command_names_or_all, FoundConfig, FoundServiceConfig,
};
pub use model::{
    CandleSetupConfig, LogEvictionConfig, ResolvedLogEvictionConfig, ServiceConfig,
    CONFIG_FILENAMES, DEFAULT_CONFIG_FILENAME, LOG_EVICTION_DEFAULTS,
};
pub use paths::{get_service_cwd, is_valid_relative_path, is_valid_root_path};
pub use validate::validate_config;
