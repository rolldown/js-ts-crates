use crate::css::CssModule;
use crate::js::JavaScriptModule;
use crate::json::JsonModule;
use crate::media::MediaModule;
use crate::module_graph_error::ModuleGraphError;
use crate::yaml::YamlModule;
use mediatype::MediaTypeBuf;
use oxc::ast::ast::BindingIdentifier;
use oxc::span::{Atom, Span};
use oxc::syntax::symbol::SymbolId;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum ImportedKind {
    Default,       // import name
    DefaultType,   // import type name
    Namespace,     // import * as name
    NamespaceType, // import type * as name
    Value,         // import { value }, import { value as name }
    ValueType,     // import { type T }, import type { T }
}

#[derive(Debug)]
pub struct ImportedSymbol {
    pub kind: ImportedKind,
    pub source_name: Option<Atom>,
    pub symbol_id: Option<SymbolId>,
    pub name: Atom,
}

impl ImportedSymbol {
    pub fn from_binding(kind: ImportedKind, binding: &BindingIdentifier) -> Self {
        Self {
            kind,
            source_name: None,
            symbol_id: binding.symbol_id.clone().into_inner(),
            name: binding.name.clone(),
        }
    }
}

#[derive(Debug)]
pub enum ImportKind {
    AsyncStatic,  // import
    AsyncDynamic, // import()
    SyncStatic,   // require()
}

#[derive(Debug)]
pub struct Import {
    pub kind: ImportKind,
    pub module_id: ModuleId,
    pub source: Atom,
    pub span: Span,
    pub symbols: Vec<ImportedSymbol>,
    pub type_only: bool,
}

#[derive(Debug)]
pub enum ExportedKind {
    Default,       // export default name
    DefaultType,   // export default T
    Namespace,     // export *, export * as name
    NamespaceType, // export type *, export type * as name
    Value,         // export name, export { name }
    ValueType,     // export type T, export { type name }
}

#[derive(Debug)]
pub struct ExportedSymbol {
    pub kind: ExportedKind,
    pub symbol_id: Option<SymbolId>,
    pub name: Atom,
}

#[derive(Debug, Default)]
pub enum ExportKind {
    #[default]
    Modern, // export
    Legacy, // module.exports, exports.name
    Native, // non-JS files
}

#[derive(Debug, Default)]
pub struct Export {
    pub kind: ExportKind,
    pub module_id: Option<ModuleId>,
    pub source: Option<Atom>,
    pub span: Option<Span>,
    pub symbols: Vec<ExportedSymbol>,
    pub type_only: bool,
}

pub type ModuleId = u32;

#[derive(Debug, Default)]
pub enum Source {
    #[default]
    Unknown,
    Audio(Box<MediaModule>),
    Css(Box<CssModule>),
    Image(Box<MediaModule>),
    JavaScript(Box<JavaScriptModule>),
    Json(Box<JsonModule>),
    Video(Box<MediaModule>),
    Yaml(Box<YamlModule>),
}

pub trait SourceParser {
    fn parse_into_module(module: &mut Module) -> Result<Source, ModuleGraphError>;
}

#[derive(Debug, Default)]
pub struct Module {
    /// List of symbols being exported, and optionally the module they came from.
    pub exports: Vec<Export>,

    /// Fragment string appended to the file path.
    pub fragment: Option<String>,

    /// Unique ID of the module.
    pub id: ModuleId,

    /// List of modules being imported from, and the symbols being imported.
    pub imports: Vec<Import>,

    /// Absolute path to the module file.
    pub path: PathBuf,

    /// Query string appended to the file path.
    pub query: Option<String>,

    /// Type of module, with associated mime and source information.
    pub source: Source,
}

impl Module {
    pub fn new(path: &Path) -> Self {
        Self {
            id: 0,
            path: path.to_owned(),
            ..Module::default()
        }
    }

    pub fn get_mime_type(&self) -> &MediaTypeBuf {
        match &self.source {
            Source::Unknown => unreachable!(),
            Source::Audio(source) => &source.mime_type,
            Source::Css(source) => &source.mime_type,
            Source::Image(source) => &source.mime_type,
            Source::JavaScript(source) => &source.mime_type,
            Source::Json(source) => &source.mime_type,
            Source::Video(source) => &source.mime_type,
            Source::Yaml(source) => &source.mime_type,
        }
    }

    /// Is the module an external file (in node modules)?
    pub fn is_external(&self) -> bool {
        self.path
            .components()
            .any(|comp| comp.as_os_str() == "node_modules")
    }

    pub(crate) fn load_and_parse_source(&mut self) -> Result<(), ModuleGraphError> {
        self.source = match self.path.extension().and_then(|ext| ext.to_str()) {
            Some("css") => CssModule::parse_into_module(self)?,
            Some("js" | "jsx" | "ts" | "tsx" | "mts" | "cts" | "mjs" | "cjs") => {
                JavaScriptModule::parse_into_module(self)?
            }
            Some("json" | "jsonc" | "json5") => JsonModule::parse_into_module(self)?,
            Some("yaml" | "yml") => YamlModule::parse_into_module(self)?,
            _ => MediaModule::parse_into_module(self)?,
        };

        Ok(())
    }
}
