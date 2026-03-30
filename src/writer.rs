use std::collections::BTreeMap;
use std::path::Path;
use chrono::Utc;
use rand::Rng;

use crate::types::Operation;

/// Generate a migration filename: m{YYYYMMDD}_{HHMMSS}_{4hex}_{slug}.rs
pub fn generate_filename(message: &str) -> String {
    let now = Utc::now();
    let date = now.format("%Y%m%d").to_string();
    let time = now.format("%H%M%S").to_string();
    let hex: String = format!("{:04x}", rand::thread_rng().r#gen::<u16>());
    let slug = message.to_lowercase().replace(' ', "_").replace(|c: char| !c.is_alphanumeric() && c != '_', "");
    format!("m{}_{}_{}_{}.rs", date, time, hex, slug)
}

/// Render the full migration file content from a list of operations.
pub fn render_migration(ops: &[Operation], _migration_name: &str) -> String {
    // Collect all (table, column) Iden variants needed
    let mut table_columns: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for op in ops {
        match op {
            Operation::AddColumn { table, column } => {
                table_columns.entry(pascal(table)).or_default().push(pascal(&column.name));
            }
            Operation::DropColumn { table, column } => {
                table_columns.entry(pascal(table)).or_default().push(pascal(&column.name));
            }
            Operation::CreateTable { table, columns, foreign_keys } => {
                let cols: Vec<String> = columns.iter().map(|c| pascal(&c.name)).collect();
                table_columns.entry(pascal(table)).or_default().extend(cols);
                for fk in foreign_keys {
                    table_columns.entry(pascal(table)).or_default().push(pascal(&fk.from_col));
                    table_columns.entry(pascal(&fk.to_table)).or_default().push(pascal(&fk.to_col));
                }
            }
            Operation::DropTable { table, columns } => {
                let cols: Vec<String> = columns.iter().map(|c| pascal(&c.name)).collect();
                table_columns.entry(pascal(table)).or_default().extend(cols);
            }
            Operation::AlterColumn { .. } | Operation::AlterColumnType { .. } => {
                // Raw SQL operations — no Iden variants needed
            }
            Operation::AddForeignKey { table, fk } => {
                // Source table needs the FK column; target table needs the referenced column
                table_columns.entry(pascal(table)).or_default().push(pascal(&fk.from_col));
                table_columns.entry(pascal(&fk.to_table)).or_default().push(pascal(&fk.to_col));
            }
            Operation::DropForeignKey { table, fk } => {
                table_columns.entry(pascal(table)).or_default().push(pascal(&fk.from_col));
                table_columns.entry(pascal(&fk.to_table)).or_default().push(pascal(&fk.to_col));
            }
            Operation::CreateIndex { table, index } => {
                for col in &index.columns {
                    table_columns.entry(pascal(table)).or_default().push(pascal(col));
                }
            }
            Operation::DropIndex { table, index } => {
                for col in &index.columns {
                    table_columns.entry(pascal(table)).or_default().push(pascal(col));
                }
            }
            Operation::RenameColumn { table, from_name, to_name } => {
                table_columns.entry(pascal(table)).or_default()
                    .extend([pascal(from_name), pascal(to_name)]);
            }
            Operation::SetDefault { .. } | Operation::DropDefault { .. } => {
                // Raw SQL operations — no Iden variants needed
            }
        }
    }

    // Deduplicate columns per table
    for cols in table_columns.values_mut() {
        cols.sort();
        cols.dedup();
    }

    let mut out = String::new();

    out.push_str("use sea_orm_migration::prelude::*;\n\n");
    out.push_str("#[derive(DeriveMigrationName)]\n");
    out.push_str("pub struct Migration;\n\n");

    // Iden enums
    for (table, cols) in &table_columns {
        out.push_str(&format!("#[derive(Iden)]\nenum {} {{\n    Table,\n", table));
        for col in cols {
            out.push_str(&format!("    {},\n", col));
        }
        out.push_str("}\n\n");
    }

    out.push_str("#[async_trait::async_trait]\n");
    out.push_str("impl MigrationTrait for Migration {\n");

    // up()
    out.push_str("    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {\n");
    for (i, op) in ops.iter().enumerate() {
        if i > 0 { out.push('\n'); }
        out.push_str(&render_up(op));
    }
    out.push_str("        Ok(())\n");
    out.push_str("    }\n\n");

    // down()
    out.push_str("    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {\n");
    for (i, op) in ops.iter().enumerate() {
        if i > 0 { out.push('\n'); }
        out.push_str(&render_down(op));
    }
    out.push_str("        Ok(())\n");
    out.push_str("    }\n");

    out.push_str("}\n");
    out
}

fn render_up(op: &Operation) -> String {
    match op {
        Operation::AddColumn { table, column } => {
            let nullable = if column.nullable { ".null()" } else { ".not_null()" };
            let pk = if column.primary_key { ".primary_key()" } else { "" };
            format!(
                "        manager\n            .alter_table(\n                Table::alter()\n                    .table({}::Table)\n                    .add_column(ColumnDef::new({}::{}).{}{}{})\n                    .to_owned(),\n            )\n            .await?;\n",
                pascal(table), pascal(table), pascal(&column.name),
                column.col_type.to_seaorm_method(), nullable, pk
            )
        }
        Operation::DropColumn { table, column } => {
            format!(
                "        manager\n            .alter_table(\n                Table::alter()\n                    .table({}::Table)\n                    .drop_column({}::{})\n                    .to_owned(),\n            )\n            .await?;\n",
                pascal(table), pascal(table), pascal(&column.name)
            )
        }
        Operation::CreateTable { table, columns, foreign_keys } => {
            let mut s = format!(
                "        manager\n            .create_table(\n                Table::create()\n                    .table({}::Table)\n                    .if_not_exists()\n",
                pascal(table)
            );
            for col in columns {
                let nullable = if col.nullable { ".null()" } else { ".not_null()" };
                let pk = if col.primary_key {
                    use crate::types::ColType;
                    let auto_inc = matches!(col.col_type, ColType::SmallInteger | ColType::Integer | ColType::BigInteger);
                    if auto_inc { ".primary_key().auto_increment()" } else { ".primary_key()" }
                } else { "" };
                s.push_str(&format!(
                    "                    .col(ColumnDef::new({}::{}).{}{}{})\n",
                    pascal(table), pascal(&col.name), col.col_type.to_seaorm_method(), nullable, pk
                ));
            }
            for fk in foreign_keys {
                s.push_str(&format!(
                    "                    .foreign_key(\n                        ForeignKey::create()\n                            .name(\"{}\")\n                            .from({}::Table, {}::{})\n                            .to({}::Table, {}::{})\n                    )\n",
                    fk.name,
                    pascal(table), pascal(table), pascal(&fk.from_col),
                    pascal(&fk.to_table), pascal(&fk.to_table), pascal(&fk.to_col)
                ));
            }
            s.push_str("                    .to_owned(),\n            )\n            .await?;\n");
            s
        }
        Operation::DropTable { table, .. } => {
            format!(
                "        manager\n            .drop_table(Table::drop().table({}::Table).to_owned())\n            .await?;\n",
                pascal(table)
            )
        }
        Operation::AlterColumn { table, column, nullable } => {
            let constraint = if *nullable { "DROP NOT NULL" } else { "SET NOT NULL" };
            format!(
                "        manager\n            .get_connection()\n            .execute_unprepared(\"ALTER TABLE \\\"{}\\\" ALTER COLUMN \\\"{}\\\" {}\")\n            .await?;\n",
                table, column, constraint
            )
        }
        Operation::AlterColumnType { table, column, to, .. } => {
            format!(
                "        // WARNING: type change may fail if existing data cannot be cast.\n        // Review before running. Edit the USING clause if a custom cast is needed.\n        manager\n            .get_connection()\n            .execute_unprepared(\"ALTER TABLE \\\"{}\\\" ALTER COLUMN \\\"{}\\\" TYPE {} USING \\\"{}\\\"::{}\") \n            .await?;\n",
                table, column, to.to_sql_type(), column, to.to_sql_type()
            )
        }
        Operation::AddForeignKey { table, fk } => {
            format!(
                "        manager\n            .create_foreign_key(\n                ForeignKey::create()\n                    .name(\"{}\")\n                    .from({}::Table, {}::{})\n                    .to({}::Table, {}::{})\n                    .to_owned(),\n            )\n            .await?;\n",
                fk.name,
                pascal(table), pascal(table), pascal(&fk.from_col),
                pascal(&fk.to_table), pascal(&fk.to_table), pascal(&fk.to_col)
            )
        }
        Operation::DropForeignKey { table, fk } => {
            format!(
                "        manager\n            .drop_foreign_key(\n                ForeignKey::drop()\n                    .table({}::Table)\n                    .name(\"{}\")\n                    .to_owned(),\n            )\n            .await?;\n",
                pascal(table), fk.name
            )
        }
        Operation::CreateIndex { table, index } => {
            let unique_call = if index.unique { "\n                    .unique()" } else { "" };
            let col_calls: String = index.columns.iter()
                .map(|col| format!("\n                    .col({}::{})", pascal(table), pascal(col)))
                .collect();
            format!(
                "        manager\n            .create_index(\n                Index::create()\n                    .name(\"{}\")\n                    .table({}::Table){}{}\n                    .to_owned(),\n            )\n            .await?;\n",
                index.name,
                pascal(table), col_calls,
                unique_call
            )
        }
        Operation::DropIndex { table, index } => {
            format!(
                "        manager\n            .drop_index(\n                Index::drop()\n                    .name(\"{}\")\n                    .table({}::Table)\n                    .to_owned(),\n            )\n            .await?;\n",
                index.name, pascal(table)
            )
        }
        Operation::RenameColumn { table, from_name, to_name } => {
            format!(
                "        manager\n            .alter_table(\n                Table::alter()\n                    .table({}::Table)\n                    .rename_column({}::{}, {}::{})\n                    .to_owned(),\n            )\n            .await?;\n",
                pascal(table), pascal(table), pascal(from_name), pascal(table), pascal(to_name)
            )
        }
        Operation::SetDefault { table, column, value } => {
            format!(
                "        manager\n            .get_connection()\n            .execute_unprepared(\"ALTER TABLE \\\"{}\\\" ALTER COLUMN \\\"{}\\\" SET DEFAULT {}\")\n            .await?;\n",
                table, column, value
            )
        }
        Operation::DropDefault { table, column, .. } => {
            format!(
                "        manager\n            .get_connection()\n            .execute_unprepared(\"ALTER TABLE \\\"{}\\\" ALTER COLUMN \\\"{}\\\" DROP DEFAULT\")\n            .await?;\n",
                table, column
            )
        }
    }
}

fn render_down(op: &Operation) -> String {
    match op {
        Operation::AddColumn { table, column } => {
            // Inverse of AddColumn is DropColumn
            format!(
                "        manager\n            .alter_table(\n                Table::alter()\n                    .table({}::Table)\n                    .drop_column({}::{})\n                    .to_owned(),\n            )\n            .await?;\n",
                pascal(table), pascal(table), pascal(&column.name)
            )
        }
        Operation::DropColumn { table, column } => {
            // Inverse of DropColumn is AddColumn (reconstruct from ColumnDef)
            let nullable = if column.nullable { ".null()" } else { ".not_null()" };
            let pk = if column.primary_key { ".primary_key()" } else { "" };
            format!(
                "        manager\n            .alter_table(\n                Table::alter()\n                    .table({}::Table)\n                    .add_column(ColumnDef::new({}::{}).{}{}{})\n                    .to_owned(),\n            )\n            .await?;\n",
                pascal(table), pascal(table), pascal(&column.name),
                column.col_type.to_seaorm_method(), nullable, pk
            )
        }
        Operation::CreateTable { table, .. } => {
            // Inverse of CreateTable is DropTable
            format!(
                "        manager\n            .drop_table(Table::drop().table({}::Table).to_owned())\n            .await?;\n",
                pascal(table)
            )
        }
        Operation::DropTable { table, columns } => {
            // Inverse of DropTable is CreateTable (reconstruct from columns)
            render_up(&Operation::CreateTable { table: table.clone(), columns: columns.clone(), foreign_keys: vec![] })
        }
        Operation::AlterColumn { table, column, nullable } => {
            // Inverse: flip the nullability back
            render_up(&Operation::AlterColumn {
                table: table.clone(),
                column: column.clone(),
                nullable: !nullable,
            })
        }
        Operation::AlterColumnType { table, column, from, to } => {
            // Inverse: change back to the original type
            render_up(&Operation::AlterColumnType {
                table: table.clone(),
                column: column.clone(),
                from: to.clone(),
                to: from.clone(),
            })
        }
        Operation::AddForeignKey { table, fk } => {
            // Inverse of AddForeignKey is DropForeignKey
            render_up(&Operation::DropForeignKey { table: table.clone(), fk: fk.clone() })
        }
        Operation::DropForeignKey { table, fk } => {
            // Inverse of DropForeignKey is AddForeignKey
            render_up(&Operation::AddForeignKey { table: table.clone(), fk: fk.clone() })
        }
        Operation::CreateIndex { table, index } => {
            render_up(&Operation::DropIndex { table: table.clone(), index: index.clone() })
        }
        Operation::DropIndex { table, index } => {
            render_up(&Operation::CreateIndex { table: table.clone(), index: index.clone() })
        }
        Operation::RenameColumn { table, from_name, to_name } => {
            render_up(&Operation::RenameColumn {
                table: table.clone(),
                from_name: to_name.clone(),  // swap: down reverses the rename
                to_name: from_name.clone(),
            })
        }
        Operation::SetDefault { table, column, value } => {
            // Inverse of SetDefault is DropDefault
            render_up(&Operation::DropDefault {
                table: table.clone(),
                column: column.clone(),
                old_value: value.clone(),
            })
        }
        Operation::DropDefault { table, column, old_value } => {
            // Inverse of DropDefault is SetDefault
            render_up(&Operation::SetDefault {
                table: table.clone(),
                column: column.clone(),
                value: old_value.clone(),
            })
        }
    }
}

/// Update migration/src/lib.rs to register the new migration.
pub fn update_lib_rs(lib_path: &Path, migration_name: &str) -> anyhow::Result<()> {
    let content = std::fs::read_to_string(lib_path)
        .map_err(|_| anyhow::anyhow!("lib.rs not found at {}", lib_path.display()))?;

    // Guard against duplicate registration
    if content.contains(&format!("mod {};", migration_name)) {
        return Err(anyhow::anyhow!("Migration '{}' is already registered in lib.rs", migration_name));
    }

    let lines: Vec<&str> = content.lines().collect();

    // Find insertion point for mod declaration
    let last_mod_idx = lines.iter().rposition(|l| l.starts_with("mod "));
    let struct_idx = lines.iter().position(|l| l.contains("pub struct Migrator"));

    let mod_insert_after = match (last_mod_idx, struct_idx) {
        (Some(idx), _) => idx,
        (None, Some(idx)) => idx.saturating_sub(1),
        (None, None) => return Err(anyhow::anyhow!(
            "lib.rs format not recognized. Please add manually: mod {}; and Box::new({}::Migration),",
            migration_name, migration_name
        )),
    };

    // Find insertion point for Box::new(...) — the line with only ']'
    let vec_close_idx = lines.iter().rposition(|l| l.trim() == "]")
        .ok_or_else(|| anyhow::anyhow!(
            "lib.rs format not recognized. Could not find migrations() vector closing bracket."
        ))?;

    let mut new_lines: Vec<String> = lines.iter().map(|l| l.to_string()).collect();

    // Insert Box::new line before the closing ]
    new_lines.insert(
        vec_close_idx,
        format!("            Box::new({}::Migration),", migration_name),
    );

    // Insert mod declaration after the last mod line (indices shifted by 1 after previous insert
    // only if mod_insert_after >= vec_close_idx, but mod comes before vec so no shift needed)
    new_lines.insert(
        mod_insert_after + 1,
        format!("mod {};", migration_name),
    );

    std::fs::write(lib_path, new_lines.join("\n") + "\n")?;
    Ok(())
}

/// Convert snake_case or space-separated words to PascalCase
fn pascal(s: &str) -> String {
    s.split('_')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        })
        .collect()
}
