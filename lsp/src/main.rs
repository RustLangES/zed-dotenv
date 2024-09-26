use std::path::{Path, PathBuf};

use dashmap::DashMap;
use serde::Deserialize;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use utils::parse_dotenv;

mod utils;

/// Env_Name => (Content, Docs)
pub type Dotenv = DashMap<String, (String, Option<String>)>;

#[derive(Clone, Debug, Deserialize)]
enum DotenvLoadOrder {
    Asc,
    Desc,
}

#[derive(Clone, Debug, Deserialize)]
struct Config {
    load_shell: bool,
    item_kind: CompletionItemKind,
    eval_on_confirm: bool,
    show_documentation: bool,
    show_content_on_docs: bool,
    documentation_kind: Option<MarkupKind>,
    dotenv_environment: String,
    load_order: DotenvLoadOrder,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            load_shell: true,
            item_kind: CompletionItemKind::CONSTANT,
            eval_on_confirm: false,
            show_documentation: true,
            show_content_on_docs: true,
            documentation_kind: Some(MarkupKind::Markdown),
            dotenv_environment: ".*".to_owned(),
            load_order: DotenvLoadOrder::Desc,
        }
    }
}

#[derive(Debug)]
struct Backend {
    client: Client,
    envs: Dotenv,
    config: RwLock<Config>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            config: RwLock::default(),
            envs: Dotenv::default(),
        }
    }

    async fn get_configs(&self) -> Config {
        let config = self.config.read().await;
        config.clone()
    }

    async fn load_env_vars(&self, workdir: Option<PathBuf>, modified_file: Option<&Path>) {
        let configs = self.get_configs().await;
        let mut envs = DashMap::new();
        let regex = regex::Regex::new(&configs.dotenv_environment).unwrap();

        if configs.load_shell {
            envs.extend(std::env::vars().map(|(k, v)| (k, (v, None))));
            envs.extend(std::env::vars_os().map(|(k, v)| {
                (
                    k.to_str().unwrap().to_string(),
                    (v.to_str().unwrap().to_string(), None),
                )
            }));
        }

        // load all envfiles
        if let Some(workdir) = workdir.and_then(|v| {
            regex
                .is_match(v.file_name().unwrap().to_str().unwrap())
                .then_some(v)
        }) {
            let files = walkdir::WalkDir::new(workdir)
                .into_iter()
                .filter_map(|e| {
                    if e.as_ref().is_ok_and(|f| {
                        f.path().is_file() && regex.is_match(f.path().to_str().unwrap_or(""))
                    }) {
                        e.ok()
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for entry in files {
                let path = entry.path();
                self.client
                    .log_message(MessageType::INFO, &format!("File match: {path:?}"))
                    .await;

                let content = std::fs::read_to_string(path).unwrap();
                envs.extend(parse_dotenv(&content));
            }
        }
        // Load changed env files
        if let Some(modified_file) = modified_file.and_then(|v| {
            regex
                .is_match(v.file_name().unwrap().to_str().unwrap())
                .then_some(v)
        }) {
            let content = std::fs::read_to_string(modified_file).unwrap();
            envs.extend(parse_dotenv(&content));
        }

        self.envs.clear();
        envs.into_iter().for_each(|(key, (value, docs))| {
            self.envs
                .entry(key)
                .and_modify(|old| {
                    old.0 = value.clone();
                    old.1 = docs.clone();
                })
                .or_insert((value, docs));
        });
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(options) = params.initialization_options {
            let mut config = self.config.write().await;
            *config = serde_json::from_value(options).unwrap();
        }

        self.load_env_vars(params.root_uri.map(|root| PathBuf::from(root.path())), None)
            .await;

        Ok(InitializeResult {
            server_info: Some(ServerInfo {
                name: env!("CARGO_PKG_NAME").to_owned(),
                version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            }),
            capabilities: ServerCapabilities {
                hover_provider: None,
                completion_provider: None,
                signature_help_provider: None,
                rename_provider: None,
                inlay_hint_provider: None,
                diagnostic_provider: None,
                ..Default::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "server initialized!")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.load_env_vars(None, Some(Path::new(params.text_document.uri.path())))
            .await
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        self.load_env_vars(None, Some(Path::new(params.text_document.uri.path())))
            .await
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let configs = self.get_configs().await;
        let pos = params.text_document_position.position;
        let completions = self
            .envs
            .iter()
            .map(|env| {
                let key = env.key().to_owned();
                let (value, docs) = env.value();
                let documentation = if configs.show_documentation {
                    docs.as_ref().map(|d| {
                        let value = if configs.show_content_on_docs && !value.is_empty() {
                            format!("{d}\n\ncontent: {value}")
                        } else {
                            d.clone()
                        };
                        Documentation::MarkupContent(MarkupContent {
                            value,
                            kind: configs
                                .documentation_kind
                                .clone()
                                .unwrap_or(MarkupKind::Markdown),
                        })
                    })
                } else {
                    None
                };
                CompletionItem {
                    documentation,
                    label: key.clone(),
                    kind: Some(configs.item_kind),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit::new(
                        Range::new(pos, pos),
                        if configs.eval_on_confirm {
                            value.clone()
                        } else {
                            key
                        },
                    ))),
                    ..Default::default()
                }
            })
            .collect();
        Ok(Some(CompletionResponse::Array(completions)))
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(Backend::new);

    Server::new(stdin, stdout, socket).serve(service).await;
}
