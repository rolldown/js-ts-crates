use crate::atom::*;
use crate::css::CssModule;
use crate::dummy::DummyModule;
use crate::js::JavaScriptModule;
use crate::json::JsonModule;
use crate::media::MediaModule;
use crate::module_graph_error::ModuleGraphError;
use crate::text::TextModule;
use crate::yaml::YamlModule;
use nodejs_package_json::PackageJson;
use oxc::ast::ast::BindingIdentifier;
use oxc::span::Span;
use oxc::syntax::symbol::SymbolId;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub enum ImportedKind {
    Default,       // import name
    DefaultType,   // import type name
    Namespace,     // import * as name
    NamespaceType, // import type * as name
    Value,         // import { value }, import { value as name }
    ValueType,     // import { type T }, import type { T }
}

impl ImportedKind {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default | Self::DefaultType)
    }

    pub fn is_namespace(&self) -> bool {
        matches!(self, Self::Namespace | Self::NamespaceType)
    }

    pub fn is_type(&self) -> bool {
        matches!(
            self,
            Self::DefaultType | Self::NamespaceType | Self::ValueType
        )
    }
}

#[derive(Debug)]
pub struct ImportedSymbol {
    pub kind: ImportedKind,
    pub source_name: Option<AtomStr>,
    pub symbol_id: Option<SymbolId>,
    pub name: AtomStr,
}

impl ImportedSymbol {
    pub fn from_binding(kind: ImportedKind, binding: &BindingIdentifier) -> Self {
        Self {
            kind,
            source_name: None,
            symbol_id: binding.symbol_id.clone().into_inner(),
            name: binding.name.to_atom_str(),
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
    pub source_request: AtomStr,
    pub span: Span,
    pub symbols: Vec<ImportedSymbol>,
    pub type_only: bool,
}

impl Import {
    pub fn is_side_effect(&self) -> bool {
        !self.source_request.is_empty() && self.symbols.is_empty()
    }
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

impl ExportedKind {
    pub fn is_default(&self) -> bool {
        matches!(self, Self::Default | Self::DefaultType)
    }

    pub fn is_namespace(&self) -> bool {
        matches!(self, Self::Namespace | Self::NamespaceType)
    }

    pub fn is_type(&self) -> bool {
        matches!(
            self,
            Self::DefaultType | Self::NamespaceType | Self::ValueType
        )
    }
}

#[derive(Debug)]
pub struct ExportedSymbol {
    pub kind: ExportedKind,
    pub symbol_id: Option<SymbolId>,
    pub name: AtomStr,
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
    pub source: Option<AtomStr>,
    pub span: Option<Span>,
    pub symbols: Vec<ExportedSymbol>,
    pub type_only: bool,
}

pub type ModuleId = u32;

#[derive(Debug, Default)]
pub enum SourceKind {
    #[default]
    Unknown,
    Audio,
    Css,
    Image,
    JavaScript,
    Json,
    Text,
    Video,
    Yaml,
}

pub trait ModuleSource: fmt::Debug {
    fn kind(&self) -> SourceKind;

    fn source(&self) -> &[u8];

    fn load(
        module: &mut Module,
        package_json: Option<Arc<PackageJson>>,
    ) -> Result<Self, ModuleGraphError>
    where
        Self: Sized;

    fn parse(&mut self, _module: &mut Module) -> Result<(), ModuleGraphError> {
        Ok(())
    }
}

pub struct Module {
    /// List of symbols being exported, and optionally the module they came from.
    pub exports: Vec<Export>,

    /// Fragment string appended to the file path.
    pub fragment: Option<String>,

    /// Unique ID of the module.
    pub id: ModuleId,

    /// List of modules being imported from, and the symbols being imported.
    pub imports: Vec<Import>,

    /// Name of the package the module belongs to.
    pub package_name: Option<String>,

    /// Absolute path to the module file.
    pub path: PathBuf,

    /// Query string appended to the file path.
    pub query: Option<String>,

    /// File type specific source information.
    pub source: Box<dyn ModuleSource>,
}

impl Module {
    pub fn new(path: &Path) -> Self {
        Self {
            exports: Vec::new(),
            fragment: None,
            id: 0,
            imports: Vec::new(),
            package_name: None,
            path: path.to_owned(),
            query: None,
            source: Box::new(DummyModule),
        }
    }

    /// Return the symbol that has been exported as the default.
    pub fn get_default_exported_symbol(&self) -> Option<&ExportedSymbol> {
        for export in &self.exports {
            for symbol in &export.symbols {
                if symbol.kind.is_default() {
                    return Some(symbol);
                }
            }
        }

        None
    }

    /// Return all symbols that have been exported from all export statements.
    pub fn get_exported_symbols(&self) -> Vec<&ExportedSymbol> {
        let mut symbols = vec![];

        for export in &self.exports {
            symbols.extend(&export.symbols);
        }

        symbols
    }

    /// Is the module an external file (in node modules)?
    pub fn is_external(&self) -> bool {
        self.path
            .components()
            .any(|comp| comp.as_os_str() == "node_modules")
    }

    pub(crate) fn load_and_parse_source(
        &mut self,
        package_json: Option<Arc<PackageJson>>,
    ) -> Result<(), ModuleGraphError> {
        if let Some(package) = &package_json {
            self.package_name = package.name.clone();
        }

        // Load the file
        let mut source: Box<dyn ModuleSource> =
            match self.path.extension().and_then(|ext| ext.to_str()) {
                Some("css") => Box::new(CssModule::load(self, package_json)?),
                Some("js" | "jsx" | "ts" | "tsx" | "mts" | "cts" | "mjs" | "cjs") => {
                    Box::new(JavaScriptModule::load(self, package_json)?)
                }
                Some("json" | "jsonc" | "json5") => Box::new(JsonModule::load(self, package_json)?),
                Some("yaml" | "yml") => Box::new(YamlModule::load(self, package_json)?),
                Some(
                    "gql" | "graphql" | "html" | "less" | "map" | "sass" | "scss" | "styl" | "svg",
                ) => Box::new(TextModule::load(self, package_json)?),
                _ => Box::new(MediaModule::load(self, package_json)?),
            };

        // Parse the file then extract imports/exports
        source.parse(self)?;

        self.source = source;

        Ok(())
    }
}

impl fmt::Debug for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Primarily for snapshots on Windows
        let path = self.path.to_string_lossy().replace('\\', "/");

        f.debug_struct("Module")
            .field("exports", &self.exports)
            .field("fragment", &self.fragment)
            .field("id", &self.id)
            .field("imports", &self.imports)
            .field("package_name", &self.package_name)
            .field("path", &path)
            .field("query", &self.query)
            .field("source", &self.source)
            .finish()
    }
}
