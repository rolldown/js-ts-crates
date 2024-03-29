use crate::module_graph_error::ModuleGraphError;
use crate::{module::*, types::FxIndexMap};
use clean_path::Clean;
use nodejs_package_json::PackageJson;
use oxc_resolver::{PackageJson as ResolvedPackageJson, ResolveOptions, Resolver};
use petgraph::graphmap::GraphMap;
use petgraph::Directed;
use rustc_hash::FxHashMap;
use starbase_utils::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub enum ModuleGraphEdge {
    Import,
    Export,
}

pub type ModuleGraphType = GraphMap<ModuleId, ModuleGraphEdge, Directed>;

#[derive(Debug)]
pub struct ModuleGraph {
    pub graph: ModuleGraphType,
    pub modules: FxIndexMap<ModuleId, Arc<Module>>,
    pub packages: FxHashMap<PathBuf, Arc<PackageJson>>,
    pub resolver: Resolver,

    next_id: u32,
    paths_to_ids: FxHashMap<PathBuf, ModuleId>,
}

impl ModuleGraph {
    pub fn new() -> Self {
        Self {
            graph: GraphMap::default(),
            modules: FxIndexMap::default(),
            packages: FxHashMap::default(),
            resolver: Resolver::new(ResolveOptions {
                condition_names: vec![
                    "import".into(),
                    "module".into(),
                    "require".into(),
                    "node".into(),
                    "default".into(),
                ],
                extensions: vec![
                    ".ts".into(),
                    ".tsx".into(),
                    ".mts".into(),
                    ".cts".into(),
                    ".mjs".into(),
                    ".cjs".into(),
                    ".js".into(),
                    ".jsx".into(),
                ],
                main_fields: vec!["module".into(), "main".into()],
                ..ResolveOptions::default()
            }),
            next_id: 1, // Default/empty modules are 0
            paths_to_ids: FxHashMap::default(),
        }
    }

    pub fn load_module(
        &mut self,
        parent_dir: &Path,
        specifier: &str,
    ) -> Result<ModuleId, ModuleGraphError> {
        let resolved_path = self
            .resolver
            .resolve(parent_dir, specifier)
            .map_err(|error| ModuleGraphError::ResolveFailed {
                dir: parent_dir.to_owned(),
                specifier: specifier.to_owned(),
                error: Box::new(error),
            })?;

        self.load_module_at_path(
            resolved_path.path().clean(),
            resolved_path.query().map(|query| query.to_owned()),
            resolved_path.fragment().map(|frag| frag.to_owned()),
            resolved_path.package_json().map(Arc::clone),
        )
    }

    pub fn load_module_at_path<P: AsRef<Path>>(
        &mut self,
        path: P,
        query: Option<String>,
        fragment: Option<String>,
        package_json: Option<Arc<ResolvedPackageJson>>,
    ) -> Result<ModuleId, ModuleGraphError> {
        let resolved_path = path.as_ref();

        assert!(resolved_path.is_absolute(), "Path must be absolute!");

        // Module already exists in the graph, avoid duplicates
        if let Some(module_id) = self.paths_to_ids.get(resolved_path) {
            return Ok(*module_id);
        }

        // Load the package.json before the module
        let package_json = if let Some(json) = package_json {
            Some(self.load_package_json(&json.realpath)?)
        } else {
            None
        };

        // Generate the ID and add to the graph
        let module_id = self.graph.add_node(self.next_id);

        self.next_id += 1;
        self.paths_to_ids
            .insert(resolved_path.to_owned(), module_id);

        // Load and parse the module, then add to the graph
        let mut module = Module::new(resolved_path);
        module.id = module_id;
        module.fragment = fragment;
        module.query = query;

        module.load_and_parse_source(package_json)?;

        // Load each imported and exported module, then connect edges
        let parent_dir = resolved_path.parent().unwrap();

        for import in module.imports.iter_mut() {
            import.module_id = self.load_module(parent_dir, &import.source_request)?;

            self.graph
                .add_edge(module_id, import.module_id, ModuleGraphEdge::Import);
        }

        for export in module.exports.iter_mut() {
            let Some(source) = &export.source else {
                continue;
            };

            let dep_module_id = self.load_module(parent_dir, source)?;

            export.module_id = Some(dep_module_id);

            self.graph
                .add_edge(module_id, dep_module_id, ModuleGraphEdge::Export);
        }

        // Store the module in the graph
        self.modules.insert(module_id, Arc::new(module));

        Ok(module_id)
    }

    pub fn load_package_json<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<Arc<PackageJson>, ModuleGraphError> {
        let path = path.as_ref();

        if let Some(json) = self.packages.get(path) {
            return Ok(Arc::clone(json));
        }

        let json: PackageJson = json::read_file(path)?;
        let data = Arc::new(json);

        self.packages.insert(path.to_path_buf(), Arc::clone(&data));

        Ok(data)
    }
}
