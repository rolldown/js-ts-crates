use super::stats::JavaScriptStats;
use crate::atom::*;
use crate::module::*;
use oxc::ast::ast::{
    Argument, AssignmentTarget, BindingPattern, BindingPatternKind, CallExpression, Declaration,
    ExportAllDeclaration, ExportDefaultDeclaration, ExportDefaultDeclarationKind,
    ExportNamedDeclaration, Expression, ImportDeclaration, ImportDeclarationSpecifier,
    ImportExpression, MemberExpression, ModuleDeclaration, SimpleAssignmentTarget, Statement,
    TSModuleReference,
};
use oxc::ast::{AstKind, Visit};
use oxc::span::Span;
use rustc_hash::FxHashSet;
use std::marker::PhantomData;

pub struct ExtractImportsExports<'ast, 'module> {
    pub module: &'module mut Module,
    pub stats: &'module mut JavaScriptStats,
    pub extracted_dynamic_imports: FxHashSet<Span>,
    pub extracted_requires: FxHashSet<Span>,
    pub ast: PhantomData<&'ast ()>,
}

// TODO non-literal paths
impl<'ast, 'module> Visit<'ast> for ExtractImportsExports<'ast, 'module> {
    fn enter_node(&mut self, kind: AstKind<'ast>) {
        match kind {
            AstKind::Program(program) => {
                for stmt in &program.body {
                    if let Statement::ModuleDeclaration(decl) = &stmt {
                        if let ModuleDeclaration::ImportDeclaration(_) = &**decl {
                            self.stats.import_statements += 1;
                        } else {
                            self.stats.export_statements += 1;
                        }

                        continue;
                    }

                    if let Statement::ExpressionStatement(expr) = &stmt {
                        if let Expression::AwaitExpression(aw) = &expr.expression {
                            if let Expression::ImportExpression(_) = &aw.argument {
                                // Handle in other methods
                                continue;
                            }
                        }
                    }

                    self.stats.other_statements += 1;
                }
            }

            // module.exports = value
            AstKind::AssignmentExpression(expr) => {
                if let AssignmentTarget::SimpleAssignmentTarget(
                    SimpleAssignmentTarget::MemberAssignmentTarget(member),
                ) = &expr.left
                {
                    if member.is_specific_member_access("module", "exports") {
                        let name = match &expr.right {
                            Expression::Identifier(ident) => Some(ident.name.clone()),
                            Expression::ClassExpression(class) => {
                                class.id.as_ref().map(|id| id.name.clone())
                            }
                            Expression::FunctionExpression(func) => {
                                func.id.as_ref().map(|id| id.name.clone())
                            }
                            _ => None,
                        };

                        self.module.exports.push(Export {
                            kind: ExportKind::Legacy,
                            span: Some(expr.span),
                            symbols: vec![ExportedSymbol {
                                kind: ExportedKind::Default,
                                symbol_id: None,
                                name: name
                                    .map(|n| n.to_atom_str())
                                    .unwrap_or(AtomStr::from("default")),
                            }],
                            ..Export::default()
                        });

                        // Should we do this???
                        self.stats.exports_default = true;
                    }
                }
            }

            // require()
            AstKind::CallExpression(require) => {
                if require.callee.is_specific_id("require") && require.arguments.len() == 1 {
                    if let Argument::Expression(Expression::StringLiteral(source)) =
                        &require.arguments[0]
                    {
                        if !self.extracted_requires.contains(&require.span) {
                            self.extracted_requires.insert(require.span);

                            self.module.imports.push(Import {
                                kind: ImportKind::SyncStatic,
                                module_id: 0,
                                source_request: source.value.to_atom_str(),
                                span: require.span,
                                type_only: false,
                                symbols: vec![],
                            });
                            self.stats.require_count += 1;
                        }
                    };
                }
            }

            // export = value
            AstKind::ModuleDeclaration(ModuleDeclaration::TSExportAssignment(export)) => {
                self.module.exports.push(Export {
                    kind: ExportKind::Modern,
                    span: Some(export.span),
                    symbols: vec![ExportedSymbol {
                        kind: ExportedKind::Default,
                        symbol_id: None,
                        name: AtomStr::from("default"),
                    }],
                    ..Export::default()
                });
            }

            // exports.name = value
            AstKind::MemberExpression(MemberExpression::StaticMemberExpression(expr)) => {
                if expr.object.is_specific_id("exports") && !expr.property.name.is_empty() {
                    self.module.exports.push(Export {
                        kind: ExportKind::Legacy,
                        span: Some(expr.span),
                        symbols: vec![ExportedSymbol {
                            kind: ExportedKind::Value,
                            symbol_id: None,
                            name: expr.property.name.to_atom_str(),
                        }],
                        ..Export::default()
                    });
                }
            }

            // import value = require()
            AstKind::TSImportEqualsDeclaration(decl) => {
                if let TSModuleReference::ExternalModuleReference(ext_module) =
                    &*decl.module_reference
                {
                    self.module.imports.push(Import {
                        kind: ImportKind::SyncStatic,
                        module_id: 0,
                        source_request: ext_module.expression.value.to_atom_str(),
                        span: decl.span,
                        symbols: vec![ImportedSymbol {
                            kind: ImportedKind::Default,
                            source_name: None,
                            symbol_id: decl.id.symbol_id.clone().into_inner(),
                            name: decl.id.name.to_atom_str(),
                        }],
                        type_only: decl.import_kind.is_type(),
                    });
                }
            }

            // { .. } = await import()
            // { .. } = require()
            AstKind::VariableDeclarator(decl) => {
                let Some(init) = &decl.init else {
                    return;
                };

                // import()
                if let Some(import) = extract_dynamic_import_from_expression(init) {
                    if let Expression::StringLiteral(source) = &import.source {
                        if !self.extracted_dynamic_imports.contains(&import.span) {
                            self.extracted_dynamic_imports.insert(import.span);

                            let mut record = Import {
                                kind: ImportKind::AsyncDynamic,
                                module_id: 0,
                                source_request: source.value.to_atom_str(),
                                span: import.span,
                                type_only: false,
                                symbols: vec![],
                            };

                            import_binding_pattern(&decl.id, &mut record.symbols);

                            self.module.imports.push(record);
                            self.stats.dynamic_import_count += 1;
                        }
                    };
                }

                // require()
                if let Some(require) = extract_require_from_expression(init) {
                    if let Argument::Expression(Expression::StringLiteral(source)) =
                        &require.arguments[0]
                    {
                        if !self.extracted_requires.contains(&require.span) {
                            self.extracted_requires.insert(require.span);

                            let mut record = Import {
                                kind: ImportKind::SyncStatic,
                                module_id: 0,
                                source_request: source.value.to_atom_str(),
                                span: require.span,
                                type_only: false,
                                symbols: vec![],
                            };

                            import_binding_pattern(&decl.id, &mut record.symbols);

                            self.module.imports.push(record);
                            self.stats.require_count += 1;
                        }
                    };
                }
            }

            _ => {}
        };
    }

    // export *
    // export * as name
    // export type *
    // export type * as name
    fn visit_export_all_declaration(&mut self, export: &ExportAllDeclaration<'ast>) {
        let mut record = Export {
            kind: ExportKind::Modern,
            span: Some(export.span),
            source: Some(export.source.value.to_atom_str()),
            type_only: export.export_kind.is_type(),
            ..Export::default()
        };

        let kind = if export.export_kind.is_type() {
            ExportedKind::NamespaceType
        } else {
            ExportedKind::Namespace
        };

        if let Some(namespace) = &export.exported {
            record.symbols.push(ExportedSymbol {
                kind,
                symbol_id: None,
                name: namespace.name().to_atom_str(),
            });
        } else {
            record.symbols.push(ExportedSymbol {
                kind,
                symbol_id: None,
                name: AtomStr::from("*"),
            });
        }

        self.module.exports.push(record);
    }

    // export default
    fn visit_export_default_declaration(&mut self, export: &ExportDefaultDeclaration<'ast>) {
        let mut record = Export {
            kind: ExportKind::Modern,
            span: Some(export.span),
            type_only: export.declaration.is_typescript_syntax(),
            ..Export::default()
        };

        let ident = match &export.declaration {
            ExportDefaultDeclarationKind::ClassDeclaration(decl) => decl.id.as_ref(),
            ExportDefaultDeclarationKind::FunctionDeclaration(decl) => decl.id.as_ref(),
            ExportDefaultDeclarationKind::Expression(Expression::Identifier(ident)) => {
                record.symbols.push(ExportedSymbol {
                    kind: ExportedKind::Default,
                    symbol_id: None,
                    name: ident.name.to_atom_str(),
                });

                None
            }
            // This doesn't work...
            ExportDefaultDeclarationKind::TSEnumDeclaration(decl) => Some(&decl.id),
            ExportDefaultDeclarationKind::TSInterfaceDeclaration(decl) => Some(&decl.id),
            _ => {
                return;
            }
        };

        if record.symbols.is_empty() {
            if let Some(ident) = ident {
                record.symbols.push(ExportedSymbol {
                    kind: if export.declaration.is_typescript_syntax() {
                        ExportedKind::DefaultType
                    } else {
                        ExportedKind::Default
                    },
                    symbol_id: ident.symbol_id.clone().into_inner(),
                    name: ident.name.to_atom_str(),
                });
            } else {
                record.symbols.push(ExportedSymbol {
                    kind: ExportedKind::Default,
                    symbol_id: None,
                    name: AtomStr::from("default"),
                });
            }
        }

        self.module.exports.push(record);
        self.stats.exports_default = true;
    }

    // export { name }
    // export { type name }
    // export type { name }
    // export const name
    // export let name
    // export type name
    fn visit_export_named_declaration(&mut self, export: &ExportNamedDeclaration<'ast>) {
        let mut record = Export {
            kind: ExportKind::Modern,
            span: Some(export.span),
            type_only: export.is_typescript_syntax(),
            ..Export::default()
        };

        if let Some(decl) = &export.declaration {
            match decl {
                Declaration::VariableDeclaration(vars) => {
                    for var in &vars.declarations {
                        export_binding_pattern(&var.id, &mut record.symbols);
                    }
                }
                Declaration::FunctionDeclaration(d) => {
                    let id = d.id.as_ref().unwrap();

                    record.symbols.push(ExportedSymbol {
                        kind: ExportedKind::Value,
                        symbol_id: id.symbol_id.clone().into_inner(),
                        name: id.name.to_atom_str(),
                    });
                }
                Declaration::ClassDeclaration(d) => {
                    let id = d.id.as_ref().unwrap();

                    record.symbols.push(ExportedSymbol {
                        kind: ExportedKind::Value,
                        symbol_id: id.symbol_id.clone().into_inner(),
                        name: id.name.to_atom_str(),
                    });
                }
                Declaration::TSTypeAliasDeclaration(d) => {
                    record.symbols.push(ExportedSymbol {
                        kind: ExportedKind::ValueType,
                        symbol_id: d.id.symbol_id.clone().into_inner(),
                        name: d.id.name.to_atom_str(),
                    });
                }
                Declaration::TSInterfaceDeclaration(d) => {
                    record.symbols.push(ExportedSymbol {
                        kind: ExportedKind::ValueType,
                        symbol_id: d.id.symbol_id.clone().into_inner(),
                        name: d.id.name.to_atom_str(),
                    });
                }
                Declaration::TSEnumDeclaration(d) => {
                    record.symbols.push(ExportedSymbol {
                        kind: ExportedKind::ValueType,
                        symbol_id: d.id.symbol_id.clone().into_inner(),
                        name: d.id.name.to_atom_str(),
                    });
                }
                Declaration::TSModuleDeclaration(d) => {
                    record.symbols.push(ExportedSymbol {
                        kind: ExportedKind::ValueType,
                        symbol_id: None,
                        name: d.id.name().to_atom_str(),
                    });
                }
                Declaration::TSImportEqualsDeclaration(d) => {
                    record.symbols.push(ExportedSymbol {
                        kind: ExportedKind::ValueType,
                        symbol_id: d.id.symbol_id.clone().into_inner(),
                        name: d.id.name.to_atom_str(),
                    });
                }
                _ => {}
            };
        }

        for specifier in &export.specifiers {
            record.symbols.push(ExportedSymbol {
                kind: if export.export_kind.is_type() || specifier.export_kind.is_type() {
                    ExportedKind::ValueType
                } else {
                    ExportedKind::Value
                },
                symbol_id: None, // Is this correct?
                name: specifier.local.name().to_atom_str(),
            });
        }

        if let Some(source) = &export.source {
            record.source = Some(source.value.to_atom_str());
        }

        if !record.symbols.is_empty() {
            self.module.exports.push(record);
        }
    }

    // import default
    // import type default
    // import { name }
    // import { name, type T }
    // import type { T }
    // import * as ns
    // import type * as ns
    fn visit_import_declaration(&mut self, import: &ImportDeclaration<'ast>) {
        let mut record = Import {
            kind: ImportKind::AsyncStatic,
            module_id: 0,
            source_request: import.source.value.to_atom_str(),
            span: import.span,
            type_only: import.import_kind.is_type(),
            symbols: vec![],
        };

        if let Some(specifiers) = &import.specifiers {
            for specifier in specifiers {
                match specifier {
                    ImportDeclarationSpecifier::ImportSpecifier(spec) => {
                        let mut value = ImportedSymbol::from_binding(
                            if import.import_kind.is_type() || spec.import_kind.is_type() {
                                ImportedKind::ValueType
                            } else {
                                ImportedKind::Value
                            },
                            &spec.local,
                        );

                        let source_name = spec.imported.name();

                        if source_name.as_str() == "default" {
                            value.kind = ImportedKind::Default;
                        } else if source_name.as_str() != value.name.as_str() {
                            value.source_name = Some(source_name.to_atom_str());
                        }

                        record.symbols.push(value);
                    }
                    ImportDeclarationSpecifier::ImportDefaultSpecifier(spec) => {
                        record.symbols.push(ImportedSymbol::from_binding(
                            if import.import_kind.is_type() {
                                ImportedKind::DefaultType
                            } else {
                                ImportedKind::Default
                            },
                            &spec.local,
                        ));
                    }
                    ImportDeclarationSpecifier::ImportNamespaceSpecifier(spec) => {
                        record.symbols.push(ImportedSymbol::from_binding(
                            if import.import_kind.is_type() {
                                ImportedKind::NamespaceType
                            } else {
                                ImportedKind::Namespace
                            },
                            &spec.local,
                        ));
                    }
                };
            }
        }

        // Don't check if symbols is empty, so that we can support
        // side-effect imports like `import './all'`
        self.module.imports.push(record);
    }

    // import()
    fn visit_import_expression(&mut self, import: &ImportExpression<'ast>) {
        if let Expression::StringLiteral(source) = &import.source {
            if !self.extracted_dynamic_imports.contains(&import.span) {
                self.extracted_dynamic_imports.insert(import.span);

                self.module.imports.push(Import {
                    kind: ImportKind::AsyncDynamic,
                    module_id: 0,
                    source_request: source.value.to_atom_str(),
                    span: import.span,
                    type_only: false,
                    symbols: vec![],
                });
                self.stats.dynamic_import_count += 1;
            }
        };
    }
}

fn extract_require_from_expression<'expr, 'ast>(
    expr: &'expr Expression<'ast>,
) -> Option<&'expr CallExpression<'ast>> {
    if let Expression::CallExpression(outer) = expr {
        if outer.callee.is_specific_id("require") && outer.arguments.len() == 1 {
            return Some(outer);
        }
    }

    None
}

fn extract_dynamic_import_from_expression<'expr, 'ast>(
    expr: &'expr Expression<'ast>,
) -> Option<&'expr ImportExpression<'ast>> {
    if let Expression::AwaitExpression(outer) = expr {
        if let Expression::ImportExpression(inner) = &outer.argument {
            return Some(inner);
        }
    }

    None
}

fn import_binding_pattern(binding: &BindingPattern, list: &mut Vec<ImportedSymbol>) {
    match &binding.kind {
        // foo = import()
        BindingPatternKind::BindingIdentifier(ident) => {
            list.push(ImportedSymbol {
                kind: ImportedKind::Namespace,
                source_name: None,
                symbol_id: ident.symbol_id.clone().into_inner(),
                name: ident.name.to_atom_str(),
            });
        }

        // { a, b, ...rest } = import()
        BindingPatternKind::ObjectPattern(object) => {
            for prop in &object.properties {
                let kind = if prop.key.is_specific_id("default") {
                    ImportedKind::Default
                } else {
                    ImportedKind::Value
                };

                match &prop.value.kind {
                    BindingPatternKind::BindingIdentifier(ident) => {
                        list.push(ImportedSymbol {
                            kind,
                            source_name: if prop.key.is_specific_id(&ident.name) {
                                None
                            } else {
                                prop.key.name().map(|n| n.to_atom_str())
                            },
                            symbol_id: ident.symbol_id.clone().into_inner(),
                            name: ident.name.to_atom_str(),
                        });
                    }
                    _ => {
                        list.push(ImportedSymbol {
                            kind,
                            source_name: None,
                            symbol_id: None,
                            name: prop.key.name().map(|n| n.to_atom_str()).unwrap(),
                        });
                    }
                };
            }

            if let Some(rest) = &object.rest {
                if let BindingPatternKind::BindingIdentifier(ident) = &rest.argument.kind {
                    list.push(ImportedSymbol {
                        kind: ImportedKind::Namespace,
                        source_name: None,
                        symbol_id: None,
                        name: ident.name.to_atom_str(),
                    });
                }
            }
        }

        // [a, b] = import()
        BindingPatternKind::ArrayPattern(_) => {
            // Not possible
        }

        // { a = 1 } = import()
        BindingPatternKind::AssignmentPattern(assign) => {
            import_binding_pattern(&assign.left, list);
        }
    };
}

fn export_binding_pattern(binding: &BindingPattern, list: &mut Vec<ExportedSymbol>) {
    match &binding.kind {
        BindingPatternKind::BindingIdentifier(ident) => {
            list.push(ExportedSymbol {
                kind: ExportedKind::Value,
                symbol_id: ident.symbol_id.clone().into_inner(),
                name: ident.name.to_atom_str(),
            });
        }
        BindingPatternKind::ObjectPattern(object) => {
            for prop in &object.properties {
                export_binding_pattern(&prop.value, list);
            }

            if let Some(rest) = &object.rest {
                export_binding_pattern(&rest.argument, list);
            }
        }
        BindingPatternKind::ArrayPattern(array) => {
            for item in array.elements.iter().flatten() {
                export_binding_pattern(item, list);
            }

            if let Some(rest) = &array.rest {
                export_binding_pattern(&rest.argument, list);
            }
        }
        BindingPatternKind::AssignmentPattern(assign) => {
            export_binding_pattern(&assign.left, list);
        }
    };
}
