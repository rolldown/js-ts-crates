use crate::atom::AtomStr;
use crate::module::*;
use crate::module_graph_error::ModuleGraphError;
use nodejs_package_json::PackageJson;
use starbase_utils::{fs, json};
use std::sync::Arc;

pub use starbase_utils::json::JsonValue;

#[derive(Debug)]
pub struct JsonModule {
    pub data: Arc<JsonValue>,
    pub source: Arc<String>,
}

impl ModuleSource for JsonModule {
    fn kind(&self) -> SourceKind {
        SourceKind::Json
    }

    fn source(&self) -> &[u8] {
        self.source.as_bytes()
    }

    fn load(
        module: &mut Module,
        _package_json: Option<Arc<PackageJson>>,
    ) -> Result<Self, ModuleGraphError> {
        let source = fs::read_file(&module.path)?;
        let data: JsonValue = json::parse(&source)?;

        Ok(Self {
            data: Arc::new(data),
            source: Arc::new(source),
        })
    }

    fn parse(&mut self, module: &mut Module) -> Result<(), ModuleGraphError> {
        let mut export = Export {
            kind: ExportKind::Native,
            ..Export::default()
        };

        // The entire document itself is a default export
        export.symbols.push(ExportedSymbol {
            kind: ExportedKind::Default,
            symbol_id: None,
            name: AtomStr::from("default"),
        });

        // When an object document, each direct property is an export
        if let JsonValue::Object(object) = &*self.data {
            for key in object.keys() {
                export.symbols.push(ExportedSymbol {
                    kind: ExportedKind::Value,
                    symbol_id: None,
                    name: AtomStr::from(key.as_str()),
                });
            }
        }

        module.exports.push(export);

        Ok(())
    }
}
