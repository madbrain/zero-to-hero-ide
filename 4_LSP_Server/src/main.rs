use std::fs;
use std::path::Path;
use std::sync::Arc;

use dashmap::DashMap;
use glob::glob;
use log::{debug, warn, error};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};
use tree_sitter::{Parser, Query, QueryCursor, Tree, Node};
use ropey::Rope;

static FIND_COMPONENT_QUERY_STRING: &str = r#"
(export_statement
  decorator: (decorator
    (call_expression
      function: (identifier) @dec-name
      arguments: (arguments
        (object (pair
          key: (property_identifier) @prop-name
          value: (string (string_fragment) @prop-value))))
    )
  )
  declaration: (class_declaration name: (type_identifier) @class-name) @declaration
  (#eq? @dec-name Component)
  (#eq? @prop-name selector)
)"#;

static INOUT_QUERY_STRING: &str = r#"
(
  (decorator (call_expression function: (identifier) @dec-name))
  .
  [
    (public_field_definition name: (property_identifier) @prop-name)
    (method_definition name: (property_identifier) @prop-name)
  ]
  (#match? @dec-name "Input|Output")
)"#;

#[derive(Debug)]
struct Component {
    selector: String,
    file_url: Url,
    class_name_range: Range,
    inputs: Vec<String>,
    outputs: Vec<String>
}

fn to_position(point: tree_sitter::Point) -> Position {
    return Position {
        line: point.row as u32,
        character: point.column as u32
    }
}

fn to_range(range: tree_sitter::Range) -> Range {
    return Range {
        start: to_position(range.start_point),
        end: to_position(range.end_point)
    }
}

struct ComponentAnalyzer {
    parser: Parser,
    component_query: Query,
    selector_idx: u32,
    class_name_idx: u32,
    class_dec_idx: u32,
    inout_query: Query,
    type_idx: u32,
    prop_idx: u32,
}

impl ComponentAnalyzer {
    fn new() -> Option<Self> {
        let ts_lang = tree_sitter_typescript::language_typescript();
        let mut parser = Parser::new();
        parser.set_language(ts_lang).ok()?;
        let component_query = Query::new(ts_lang, FIND_COMPONENT_QUERY_STRING).ok()?;
        let inout_query = Query::new(ts_lang, INOUT_QUERY_STRING).ok()?;
        return Some(ComponentAnalyzer {
            parser: parser,
            selector_idx: component_query.capture_index_for_name("prop-value")?,
            class_name_idx: component_query.capture_index_for_name("class-name")?,
            class_dec_idx: component_query.capture_index_for_name("declaration")?,
            component_query: component_query,
            type_idx: inout_query.capture_index_for_name("dec-name")?,
            prop_idx: inout_query.capture_index_for_name("prop-name")?,
            inout_query: inout_query
        })
    }

    async fn analyze_file(&mut self, file_path: &Path, component_index: &Arc<DashMap<String, Component>>) {
        debug!("FILE {:?}", file_path);
    
        let contents = fs::read_to_string(file_path).expect("Should have been able to read the file");
        let tree = self.parser.parse(&contents, None).unwrap();
    
        let mut component_query_cursor = QueryCursor::new();
        let component_matches = component_query_cursor.matches(&self.component_query, tree.root_node(), contents.as_bytes());
    
        for component_match in component_matches {
            let classname_node = component_match.nodes_for_capture_index(self.class_name_idx).next().unwrap();
            let declaration = component_match.nodes_for_capture_index(self.class_dec_idx).next().unwrap();
    
            let selector = component_match.nodes_for_capture_index(self.selector_idx).next()
                .and_then(|x| x.utf8_text(&contents.as_bytes()).ok()).unwrap();
            let class_name = classname_node.utf8_text(&contents.as_bytes()).ok().unwrap();
            debug!("COMP {:?} -> {:?}",selector, class_name);
    
            let mut inputs: Vec<String> = Vec::new();
            let mut outputs: Vec<String> = Vec::new();
            let mut inout_query_cursor = QueryCursor::new();
            let inout_matches = inout_query_cursor.matches(&self.inout_query, declaration, contents.as_bytes());
            for inout_match in inout_matches {
                let prop_type = inout_match.nodes_for_capture_index(self.type_idx).next()
                    .and_then(|node| node.utf8_text(&contents.as_bytes()).ok()).unwrap();
                let prop_name = inout_match.nodes_for_capture_index(self.prop_idx).next()
                    .and_then(|node| node.utf8_text(&contents.as_bytes()).ok()).unwrap();
                if prop_type.eq("Input") {
                    inputs.push(String::from(prop_name));
                } else {
                    outputs.push(String::from(prop_name));
                }
                debug!("  PROP {:?} {:?}", prop_type, prop_name);
            }
            let base_url = Url::parse("file://");
            let options = Url::options().base_url(base_url.as_ref().ok());
            let component = Component {
                selector: String::from(selector),
                file_url: file_path.to_str().and_then(|s| options.parse(s).ok()).unwrap(),
                class_name_range: to_range(classname_node.range()),
                inputs: inputs,
                outputs: outputs
            };
            component_index.insert(component.selector.clone(), component);
        }
    }

    async fn analyze_workspace(&mut self, workspace_root: &str, component_index: &Arc<DashMap<String, Component>>) {
        debug!("WORKSPACE {:?}", workspace_root);
        match glob((String::from(workspace_root) + "/src/**/*.ts").as_str()) {
            Ok(pattern) => {
                for entry in pattern {
                    match entry {
                        Ok(path) => self.analyze_file(path.as_path(), &component_index).await,
                        Err(e) => warn!("Error getting file {:?}", e),
                    }
                }
            }
            Err(e) => {
                warn!("Error making glob pattern {:?}", e);
            }
        }
    }
}

struct HtmlAnalyzer {
    parser: Parser,
}

impl HtmlAnalyzer {
    fn new() -> HtmlAnalyzer {
        let html_lang = tree_sitter_html::language();
        let mut parser = Parser::new();
        parser.set_language(html_lang).unwrap();
        return HtmlAnalyzer {
            parser: parser
        };
    }
}

fn find_node<'a>(node: &Node<'a>, offset: usize, types: Vec<&str>) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    loop {
        if cursor.node().start_byte() <= offset && offset <= cursor.node().end_byte() {
            if types.contains(&cursor.node().kind()) {
                return Some(cursor.node());
            }
            if ! cursor.goto_first_child() {
                return None;
            }
        } else {
            if ! cursor.goto_next_sibling() {
                return None;
            }
        }
    }
}

fn completion(node: &Node, offset: usize, rope: &Rope, components: &Arc<DashMap<String, Component>>) -> Vec<CompletionItem> {
    find_node(node, offset, vec![ "start_tag", "self_closing_tag" ])
        .and_then(|start_tag| {
            let tag_name = find_node(&start_tag, offset, vec![ "tag_name" ]);
            if tag_name.is_some() {
                Some(components.iter()
                    .map(|component| CompletionItem {
                        label: component.selector.clone(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        ..Default::default()
                    })
                    .collect())
            } else {
                let attr_name = find_node(&start_tag, offset, vec![ "attribute_name" ]);
                fn make_completions(elements: &Vec<String>, template: &str) -> Vec<CompletionItem> {
                    elements.iter().map(|input| CompletionItem {
                        label: input.clone(),
                        kind: Some(CompletionItemKind::FIELD),
                        insert_text: Some(template.replace("{}", input)),
                        insert_text_format: Some(InsertTextFormat::SNIPPET),
                        ..Default::default()
                    }).collect()
                }
                attr_name.and_then(|_| start_tag.named_child(0))
                    .and_then(|node| rope.slice(node.range().start_byte..node.range().end_byte).as_str())
                    .and_then(|tag_name| components.get(tag_name))
                    .and_then(|component| {
                        let mut completions = make_completions(&component.inputs, "[{}]=\"$0\"");
                        completions.append(&mut make_completions(&component.outputs, "({})=\"$0\""));
                        Some(completions)
                    })
            }
        })
        .unwrap_or(Vec::new())
}

struct Backend {
    client: Client,
    components: Arc<DashMap<String, Component>>,
    document_map: DashMap<String, Rope>,
    ast_map: DashMap<String, Tree>
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        if let Some(workspaces) = params.workspace_folders {
            let component_index = self.components.clone();
            tokio::spawn(async move {
                match ComponentAnalyzer::new() {
                    Some(mut analyzer) => analyzer.analyze_workspace(workspaces[0].uri.path(), &component_index).await,
                    None => error!("Error building analyzer")
                }
            });
        }
        Ok(InitializeResult {
            server_info: None,
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                workspace: Some(WorkspaceServerCapabilities {
                    workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                        supported: Some(true),
                        change_notifications: Some(OneOf::Left(true)),
                    }),
                    file_operations: None,
                }),
                definition_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    trigger_characters: None,
                    work_done_progress_options: Default::default(),
                    all_commit_characters: None,
                }),
                ..ServerCapabilities::default()
            },
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "initialized!")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_change_workspace_folders(&self, _: DidChangeWorkspaceFoldersParams) {
        self.client
            .log_message(MessageType::INFO, "workspace folders changed!")
            .await;
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        let mut message = String::from("watched: ");
        message.push_str(
            params
                .changes
                .iter()
                .map(|x| x.uri.to_string())
                .collect::<Vec<_>>()
                .join("|")
                .as_str(),
        );
        self.client.log_message(MessageType::INFO, message).await;
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let definition = || -> Option<GotoDefinitionResponse> {
            let uri = params.text_document_position_params.text_document.uri;
            let ast = self.ast_map.get(&uri.to_string())?;
            let rope = self.document_map.get(&uri.to_string())?;

            let position = params.text_document_position_params.position;
            let line_position = rope.try_line_to_char(position.line as usize).ok()?;
            let offset = line_position + position.character as usize;
            
            let node = find_node(&ast.root_node(), offset, vec!["tag_name"])?;
            let tag_name = rope.slice(node.start_byte()..node.end_byte()).as_str()?;
            let component = self.components.get(tag_name)?;
            return Some(GotoDefinitionResponse::Scalar(Location::new(component.file_url.clone(), component.class_name_range)));
        }();
        Ok(definition)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let completions = || -> Option<Vec<CompletionItem>> {
            let rope = self.document_map.get(&uri.to_string())?;
            let ast = self.ast_map.get(&uri.to_string())?;
            let char = rope.try_line_to_char(position.line as usize).ok()?;
            let offset = char + position.character as usize;
            return Some(completion(&ast.root_node(), offset, &rope, &self.components));
        }();
        Ok(completions.map(CompletionResponse::Array))
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file opened!")
            .await;
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: params.text_document.text,
        })
        .await
    }

    async fn did_change(&self, mut params: DidChangeTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file changed!")
            .await;
        self.on_change(TextDocumentItem {
            uri: params.text_document.uri,
            text: std::mem::take(&mut params.content_changes[0].text),
        })
        .await
    }

    async fn did_save(&self, _: DidSaveTextDocumentParams) {
        self.client
            .log_message(MessageType::INFO, "file saved!")
            .await;
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.document_map.remove(&params.text_document.uri.to_string());
        self.client
            .log_message(MessageType::INFO, "file closed!")
            .await;
    }
}

struct TextDocumentItem {
    uri: Url,
    text: String,
}

impl Backend {
    async fn on_change(&self, params: TextDocumentItem) {
        let rope = ropey::Rope::from_str(&params.text);
        self.document_map.insert(params.uri.to_string(), rope.clone());
        let tree = self.parse_html(&params.text);
        self.ast_map.insert(params.uri.to_string(), tree);
    }

    fn parse_html(&self, content: &String) -> Tree {
        let mut html_analyzer = HtmlAnalyzer::new();
        return html_analyzer.parser.parse(content, None).unwrap();
    }
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(|client| Backend {
        client,
        components: Arc::new(DashMap::new()),
        ast_map: DashMap::new(),
        document_map: DashMap::new(),
    })
    .finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}
