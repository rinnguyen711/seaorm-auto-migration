use std::path::Path;
use syn::{visit::Visit, Fields, File, ItemStruct, Type};
use walkdir::WalkDir;

use crate::types::{ColType, ColumnDef, EntitySchema, ForeignKeyDef, IndexDef};

/// Parse entities from a Rust source string directly (useful for tests).
pub fn parse_entities_from_str(src: &str) -> Vec<EntitySchema> {
    let file: File = match syn::parse_str(src) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Warning: could not parse source string: {}", e);
            return vec![];
        }
    };

    let mut visitor = EntityVisitor::default();
    visitor.visit_file(&file);

    // Attach pending FKs to the last schema (same logic as parse_entities)
    if let Some(schema) = visitor.schemas.last_mut() {
        for mut fk in visitor.pending_fks.drain(..) {
            fk.name = format!("fk_{}_{}", schema.table, fk.from_col);
            schema.foreign_keys.push(fk);
        }
    }

    visitor.schemas
}

pub fn parse_entities(dir: &Path) -> anyhow::Result<Vec<EntitySchema>> {
    let mut schemas = Vec::new();

    for entry in WalkDir::new(dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let content = std::fs::read_to_string(entry.path())?;
        let file: File = match syn::parse_str(&content) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Warning: could not parse {}: {}", entry.path().display(), e);
                continue;
            }
        };

        let mut visitor = EntityVisitor::default();
        visitor.visit_file(&file);

        if visitor.schemas.len() > 1 {
            eprintln!(
                "Warning: {} contains multiple entity models — FKs will be attached to the last one only.",
                entry.path().display()
            );
        }

        // Attach pending FKs to the schema parsed from this file.
        // Standard SeaORM layout: one DeriveEntityModel struct per file.
        if let Some(schema) = visitor.schemas.last_mut() {
            for mut fk in visitor.pending_fks.drain(..) {
                fk.name = format!("fk_{}_{}", schema.table, fk.from_col);
                schema.foreign_keys.push(fk);
            }
        }
        schemas.extend(visitor.schemas);
    }

    Ok(schemas)
}

#[derive(Default)]
struct EntityVisitor {
    schemas: Vec<EntitySchema>,
    pending_fks: Vec<ForeignKeyDef>,
}

impl<'ast> Visit<'ast> for EntityVisitor {
    fn visit_item_struct(&mut self, node: &'ast ItemStruct) {
        if !has_derive(node, "DeriveEntityModel") {
            return;
        }
        let Some(table_name) = extract_table_name(node) else { return };
        let (columns, mut field_indexes) = extract_columns(node);
        if !columns.is_empty() {
            let struct_indexes = extract_struct_indexes(node);

            // Overlap detection: warn and skip single-column struct indexes duplicating a field-level index
            let field_indexed_cols: std::collections::HashSet<String> = field_indexes
                .iter()
                .flat_map(|idx| idx.columns.iter().cloned())
                .collect();
            let filtered_struct_indexes: Vec<IndexDef> = struct_indexes.into_iter().filter(|idx| {
                if idx.columns.len() == 1 && field_indexed_cols.contains(&idx.columns[0]) {
                    eprintln!(
                        "Warning: struct-level index {:?} duplicates field-level unique/indexed — skipping",
                        idx.columns
                    );
                    false
                } else {
                    true
                }
            }).collect();
            field_indexes.extend(filtered_struct_indexes);

            // Resolve names for all indexes
            let indexes = field_indexes.into_iter().map(|mut idx| {
                let suffix = if idx.unique { "_unique" } else { "" };
                idx.name = format!("idx_{}_{}{}", table_name, idx.columns.join("_"), suffix);
                idx
            }).collect();

            self.schemas.push(EntitySchema { table: table_name, columns, foreign_keys: vec![], indexes });
        }
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        use syn::{Expr, Lit, Meta};

        // Only process enums with #[derive(DeriveRelation)]
        let has_derive_relation = node.attrs.iter().any(|attr| {
            if !attr.path().is_ident("derive") { return false; }
            attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated
            )
            .map(|paths| paths.iter().any(|p| p.is_ident("DeriveRelation")))
            .unwrap_or(false)
        });
        if !has_derive_relation { return; }

        for variant in &node.variants {
            let mut belongs_to_found = false;
            let mut from_raw: Option<String> = None;
            let mut to_raw: Option<String> = None;

            for attr in &variant.attrs {
                if !attr.path().is_ident("sea_orm") { continue; }
                let Ok(nested) = attr.parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated
                ) else { continue; };

                for meta in &nested {
                    if let Meta::NameValue(nv) = meta {
                        if let Expr::Lit(expr_lit) = &nv.value {
                            if let Lit::Str(s) = &expr_lit.lit {
                                let val = s.value();
                                if nv.path.is_ident("belongs_to") {
                                    belongs_to_found = true;
                                } else if nv.path.is_ident("from") {
                                    from_raw = Some(val);
                                } else if nv.path.is_ident("to") {
                                    to_raw = Some(val);
                                }
                            }
                        }
                    }
                }
            }

            // Skip has_many and other non-belongs_to variants
            if !belongs_to_found { continue; }
            let (Some(from_str), Some(to_str)) = (from_raw, to_raw) else { continue; };

            // "Column::UserId" → last segment → pascal_to_snake → "user_id"
            let from_col = from_str.split("::").last()
                .map(pascal_to_snake)
                .unwrap_or_default();

            // "super::users::Column::Id" → segment before "Column" → "users"
            //                            → last segment → pascal_to_snake → "id"
            let to_parts: Vec<&str> = to_str.split("::").collect();
            let col_idx = to_parts.iter().position(|s| *s == "Column");
            // Assumes the Rust module name matches the SQL table name (standard SeaORM layout).
            // e.g. "super::users::Column::Id" → to_table = "users"
            let (to_table, to_col) = match col_idx {
                Some(idx) if idx > 0 => (
                    to_parts[idx - 1].to_string(),
                    pascal_to_snake(to_parts.last().unwrap_or(&"")),
                ),
                _ => {
                    eprintln!(
                        "Warning: could not parse FK 'to' path '{}' — expected format 'module::Column::FieldName', skipping.",
                        to_str
                    );
                    continue;
                }
            };

            self.pending_fks.push(ForeignKeyDef {
                name: String::new(), // filled in post-processing once we know the table name
                from_col,
                to_table,
                to_col,
            });
        }
    }
}

/// Converts PascalCase to snake_case for SeaORM column variant names.
/// Assumes standard PascalCase — consecutive uppercase runs (e.g. `UserID`) are not supported
/// and will produce `user_i_d`. SeaORM generated code always uses standard PascalCase.
fn pascal_to_snake(s: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    for (i, &ch) in chars.iter().enumerate() {
        if ch.is_uppercase() && i > 0 && !chars[i - 1].is_uppercase() {
            out.push('_');
        }
        out.push(ch.to_lowercase().next().unwrap());
    }
    out
}

fn has_derive(node: &ItemStruct, name: &str) -> bool {
    node.attrs.iter().any(|attr| {
        if !attr.path().is_ident("derive") {
            return false;
        }
        attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated
        )
        .map(|paths| paths.iter().any(|p| p.is_ident(name)))
        .unwrap_or(false)
    })
}

fn extract_table_name(node: &ItemStruct) -> Option<String> {
    use syn::{Expr, Lit, Meta};
    for attr in &node.attrs {
        if !attr.path().is_ident("sea_orm") {
            continue;
        }
        let Ok(nested) = attr.parse_args_with(
            syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated
        ) else {
            continue;
        };
        for meta in nested {
            if let Meta::NameValue(nv) = meta {
                if nv.path.is_ident("table_name") {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let Lit::Str(s) = &expr_lit.lit {
                            return Some(s.value());
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_columns(node: &ItemStruct) -> (Vec<ColumnDef>, Vec<IndexDef>) {
    let Fields::Named(fields) = &node.fields else { return (vec![], vec![]) };
    let mut columns = Vec::new();
    let mut partial_indexes: Vec<IndexDef> = Vec::new();

    for field in &fields.named {
        let Some(ident) = &field.ident else { continue };
        let name = ident.to_string();

        // Parse all flags from #[sea_orm(...)] on this field
        let mut primary_key = false;
        let mut unique = false;
        let mut indexed = false;

        for attr in &field.attrs {
            if !attr.path().is_ident("sea_orm") { continue; }
            if let Ok(metas) = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated
            ) {
                for meta in &metas {
                    if meta.path().is_ident("primary_key") { primary_key = true; }
                    if meta.path().is_ident("unique") { unique = true; }
                    if meta.path().is_ident("indexed") { indexed = true; }
                }
            }
        }

        let (rust_type, nullable) = unwrap_option(&field.ty);

        match ColType::from_rust_type(&rust_type) {
            Some(col_type) => {
                columns.push(ColumnDef { name: name.clone(), col_type, nullable, primary_key, unique, indexed });
                // Build partial IndexDef (table name filled in visit_item_struct)
                // unique takes precedence — a unique field always gets a unique index
                if unique {
                    partial_indexes.push(IndexDef {
                        name: name.clone(), // placeholder — becomes idx_{table}_{col}_unique
                        columns: vec![name.clone()],
                        unique: true,
                    });
                } else if indexed {
                    partial_indexes.push(IndexDef {
                        name: name.clone(), // placeholder — becomes idx_{table}_{col}
                        columns: vec![name.clone()],
                        unique: false,
                    });
                }
            }
            None => eprintln!("Warning: unsupported type '{}' on '{}' — skipping", rust_type, name),
        }
    }

    (columns, partial_indexes)
}

fn extract_struct_indexes(node: &syn::ItemStruct) -> Vec<IndexDef> {
    use syn::{Meta, Expr, Lit};
    use syn::punctuated::Punctuated;
    use syn::Token;

    let mut result = Vec::new();

    for attr in &node.attrs {
        if !attr.path().is_ident("sea_orm") {
            continue;
        }
        let args = match attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) {
            Ok(a) => a,
            Err(_) => continue,
        };
        for meta in &args {
            let Meta::List(list) = meta else { continue };
            if !list.path.is_ident("indexes") {
                continue;
            }
            // parse each index(...) inside indexes(...)
            let index_metas = match list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) {
                Ok(m) => m,
                Err(_) => continue,
            };
            for index_meta in &index_metas {
                let Meta::List(index_list) = index_meta else { continue };
                if !index_list.path.is_ident("index") {
                    continue;
                }
                let index_args = match index_list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated) {
                    Ok(a) => a,
                    Err(_) => continue,
                };
                let mut columns: Vec<String> = Vec::new();
                let mut unique = false;
                for arg in &index_args {
                    match arg {
                        Meta::NameValue(nv) if nv.path.is_ident("columns") => {
                            if let Expr::Array(arr) = &nv.value {
                                for elem in &arr.elems {
                                    if let Expr::Lit(expr_lit) = elem {
                                        if let Lit::Str(s) = &expr_lit.lit {
                                            columns.push(s.value());
                                        }
                                    }
                                }
                            }
                        }
                        Meta::NameValue(nv) if nv.path.is_ident("unique") => {
                            if let Expr::Lit(expr_lit) = &nv.value {
                                if let Lit::Bool(b) = &expr_lit.lit {
                                    unique = b.value;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                if !columns.is_empty() {
                    result.push(IndexDef {
                        name: String::new(), // resolved later
                        columns,
                        unique,
                    });
                }
            }
        }
    }
    result
}

fn unwrap_option(ty: &Type) -> (String, bool) {
    let s = quote::quote!(#ty).to_string().replace(' ', "");
    if s.starts_with("Option<") && s.ends_with('>') {
        return (s[7..s.len() - 1].to_string(), true);
    }
    (s, false)
}
