use crate::types::{ColumnDef, EntitySchema, Operation, TableSchema};

pub struct DiffResult {
    pub ops: Vec<Operation>,
    pub destructive_skipped: usize,
}

/// Compute the list of operations needed to bring the DB schema in line with the entities.
/// If `allow_destructive` is false, DropColumn, DropTable, DropForeignKey, and DropIndex are excluded but counted.
pub fn compute_diff(
    entities: &[EntitySchema],
    db_tables: &[TableSchema],
    allow_destructive: bool,
    ask_rename: impl Fn(&str, &str, &str) -> bool,
) -> DiffResult {
    let mut ops = Vec::new();
    let mut destructive_skipped = 0usize;

    // Index DB tables by name for fast lookup
    let db_map: std::collections::HashMap<&str, &TableSchema> =
        db_tables.iter().map(|t| (t.table.as_str(), t)).collect();

    // Index entity tables by name for orphan detection
    let entity_names: std::collections::HashSet<&str> =
        entities.iter().map(|e| e.table.as_str()).collect();

    // --- Per-entity diff ---
    for entity in entities {
        match db_map.get(entity.table.as_str()) {
            None => {
                // Table doesn't exist in DB → CreateTable
                ops.push(Operation::CreateTable {
                    table: entity.table.clone(),
                    columns: entity.columns.clone(),
                    foreign_keys: entity.foreign_keys.clone(),
                });
            }
            Some(db_table) => {
                let db_col_map: std::collections::HashMap<&str, &ColumnDef> =
                    db_table.columns.iter().map(|c| (c.name.as_str(), c)).collect();

                let entity_col_names: std::collections::HashSet<&str> =
                    entity.columns.iter().map(|c| c.name.as_str()).collect();

                // --- Column diff ---
                // Phase 1: collect added/dropped without pushing ops.
                // AlterColumnType and AlterColumn are pushed immediately (not rename candidates).
                let mut added: Vec<ColumnDef> = Vec::new();
                let mut dropped: Vec<ColumnDef> = Vec::new();

                for col in &entity.columns {
                    match db_col_map.get(col.name.as_str()) {
                        None => added.push(col.clone()),
                        Some(db_col) => {
                            if db_col.col_type != col.col_type {
                                ops.push(Operation::AlterColumnType {
                                    table: entity.table.clone(),
                                    column: col.name.clone(),
                                    from: db_col.col_type.clone(),
                                    to: col.col_type.clone(),
                                });
                            }
                            if db_col.primary_key != col.primary_key {
                                eprintln!(
                                    "Primary key change detected on {}.{} — not supported in v1, migrate manually.",
                                    entity.table, col.name
                                );
                            }
                            if db_col.nullable != col.nullable {
                                ops.push(Operation::AlterColumn {
                                    table: entity.table.clone(),
                                    column: col.name.clone(),
                                    nullable: col.nullable,
                                });
                            }
                        }
                    }
                }

                for db_col in &db_table.columns {
                    if !entity_col_names.contains(db_col.name.as_str()) {
                        dropped.push(db_col.clone());
                    }
                }

                // Phase 2: rename detection.
                // consumed tracks column names (both from_name and to_name) that have been matched.
                // Only updated on confirm — declined prompts do not add to consumed.
                // ask_rename is called regardless of allow_destructive.
                let mut consumed: std::collections::HashSet<String> = std::collections::HashSet::new();

                for dropped_col in &dropped {
                    if consumed.contains(&dropped_col.name) { continue; }

                    let candidates: Vec<&ColumnDef> = added.iter()
                        .filter(|a| {
                            !consumed.contains(&a.name)
                                && a.col_type == dropped_col.col_type
                                && a.nullable == dropped_col.nullable
                                && a.primary_key == dropped_col.primary_key
                        })
                        .collect();

                    for candidate in candidates {
                        if ask_rename(&entity.table, &dropped_col.name, &candidate.name) {
                            ops.push(Operation::RenameColumn {
                                table: entity.table.clone(),
                                from_name: dropped_col.name.clone(),
                                to_name: candidate.name.clone(),
                            });
                            consumed.insert(dropped_col.name.clone());
                            consumed.insert(candidate.name.clone());
                            break;
                        }
                    }
                }

                // Phase 3: push remaining ops.
                for col in &dropped {
                    if consumed.contains(&col.name) { continue; }
                    if allow_destructive {
                        ops.push(Operation::DropColumn {
                            table: entity.table.clone(),
                            column: col.clone(),
                        });
                    } else {
                        eprintln!(
                            "Warning: skipping DropColumn {}.{} (re-run without --no-destructive to include)",
                            entity.table, col.name
                        );
                        destructive_skipped += 1;
                    }
                }

                for col in &added {
                    if consumed.contains(&col.name) { continue; }
                    ops.push(Operation::AddColumn {
                        table: entity.table.clone(),
                        column: col.clone(),
                    });
                }

                // --- FK diff ---
                // Match FKs by (from_col, to_table, to_col) — not by name, since the DB
                // may use a different constraint name than the entity convention.
                type FkKey<'a> = (&'a str, &'a str, &'a str);
                let db_fk_keys: std::collections::HashSet<FkKey> = db_table.foreign_keys
                    .iter()
                    .map(|f| (f.from_col.as_str(), f.to_table.as_str(), f.to_col.as_str()))
                    .collect();

                for fk in &entity.foreign_keys {
                    let key = (fk.from_col.as_str(), fk.to_table.as_str(), fk.to_col.as_str());
                    if !db_fk_keys.contains(&key) {
                        ops.push(Operation::AddForeignKey {
                            table: entity.table.clone(),
                            fk: fk.clone(),
                        });
                    }
                }

                // FKs in DB not in entity → DropForeignKey (destructive)
                let entity_fk_keys: std::collections::HashSet<FkKey> = entity.foreign_keys
                    .iter()
                    .map(|f| (f.from_col.as_str(), f.to_table.as_str(), f.to_col.as_str()))
                    .collect();

                for db_fk in &db_table.foreign_keys {
                    let key = (db_fk.from_col.as_str(), db_fk.to_table.as_str(), db_fk.to_col.as_str());
                    if !entity_fk_keys.contains(&key) {
                        if allow_destructive {
                            ops.push(Operation::DropForeignKey {
                                table: entity.table.clone(),
                                fk: db_fk.clone(),
                            });
                        } else {
                            eprintln!(
                                "Warning: skipping DropForeignKey {}.{} (re-run without --no-destructive to include)",
                                entity.table, db_fk.name
                            );
                            destructive_skipped += 1;
                        }
                    }
                }

                // --- Index diff ---
                // Match indexes by (columns, unique) — not by name, since the DB may use a
                // different name (e.g. unique constraint "users_email_key" vs index
                // "idx_users_email_unique"). Two indexes on the same columns with the same
                // uniqueness are considered the same index.
                type IdxKey = (Vec<String>, bool);
                let db_idx_keys: std::collections::HashSet<IdxKey> = db_table.indexes
                    .iter()
                    .map(|i| (i.columns.clone(), i.unique))
                    .collect();

                for index in &entity.indexes {
                    let key = (index.columns.clone(), index.unique);
                    if !db_idx_keys.contains(&key) {
                        ops.push(Operation::CreateIndex {
                            table: entity.table.clone(),
                            index: index.clone(),
                        });
                    }
                }

                // Indexes in DB not in entity → DropIndex (destructive)
                let entity_idx_keys: std::collections::HashSet<IdxKey> = entity.indexes
                    .iter()
                    .map(|i| (i.columns.clone(), i.unique))
                    .collect();

                for db_idx in &db_table.indexes {
                    let key = (db_idx.columns.clone(), db_idx.unique);
                    if !entity_idx_keys.contains(&key) {
                        if allow_destructive {
                            ops.push(Operation::DropIndex {
                                table: entity.table.clone(),
                                index: db_idx.clone(),
                            });
                        } else {
                            eprintln!(
                                "Warning: skipping DropIndex {}.{} (re-run without --no-destructive to include)",
                                entity.table, db_idx.name
                            );
                            destructive_skipped += 1;
                        }
                    }
                }
            }
        }
    }

    // --- Orphan tables in DB not in entities → DropTable (destructive) ---
    for db_table in db_tables {
        // Skip SeaORM's own migration tracking table
        if db_table.table == "seaql_migrations" { continue; }

        if !entity_names.contains(db_table.table.as_str()) {
            if allow_destructive {
                ops.push(Operation::DropTable {
                    table: db_table.table.clone(),
                    columns: db_table.columns.clone(),
                });
            } else {
                eprintln!(
                    "Warning: skipping DropTable {} (re-run without --no-destructive to include)",
                    db_table.table
                );
                destructive_skipped += 1;
            }
        }
    }

    if destructive_skipped > 0 {
        eprintln!(
            "Skipped {} destructive operation(s). Re-run without --no-destructive to include them.",
            destructive_skipped
        );
    }

    DiffResult { ops, destructive_skipped }
}
