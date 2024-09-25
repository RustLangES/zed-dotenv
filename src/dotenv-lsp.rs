use std::fs;
use zed::LanguageServerId;
use zed_extension_api::{self as zed, Result};

struct DotEnvEntension {
    cached_binary_path: Option<String>,
}

impl DotEnvEntension {
    fn language_server_binary_path(
        &mut self,
        language_server_id: &LanguageServerId,
    ) -> Result<String> {
        if let Some(path) = &self.cached_binary_path {
            if fs::metadata(path).map_or(false, |stat| stat.is_file()) {
                return Ok(path.clone());
            }
        }

        zed::set_language_server_installation_status(
            language_server_id,
            &zed::LanguageServerInstallationStatus::CheckingForUpdate,
        );

        // TODO: Implement logic to find or install the language server binary
        self.cached_binary_path = Some("path/to/dotenv-lsp".to_string());
        Ok(self.cached_binary_path.clone().unwrap())
    }
}

impl zed::Extension for DotEnvEntension {
    fn new() -> Self {
        Self {
            cached_binary_path: None,
        }
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        _worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        Ok(zed::Command {
            command: self.language_server_binary_path(language_server_id)?,
            args: vec!["--stdio".to_string()],
            env: Default::default(),
        })
    }
}

zed::register_extension!(DotEnvEntension);
