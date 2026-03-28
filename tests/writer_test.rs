use seaorm_auto_migration::types::*;
use seaorm_auto_migration::writer::{generate_filename, render_migration, update_lib_rs};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_filename_format() {
    let name = generate_filename("add desc to posts");
    // m20260325_143000_4f2a_add_desc_to_posts.rs
    assert!(name.starts_with('m'));
    assert!(name.ends_with(".rs"));
    let parts: Vec<&str> = name.trim_end_matches(".rs").splitn(4, '_').collect();
    assert_eq!(parts.len(), 4);
    assert_eq!(parts[0].len(), 9); // mYYYYMMDD
    assert_eq!(parts[1].len(), 6); // HHMMSS
    assert_eq!(parts[2].len(), 4); // hex suffix
    assert_eq!(parts[3], "add_desc_to_posts"); // slug
}

#[test]
fn test_render_add_column() {
    let ops = vec![Operation::AddColumn {
        table: "posts".to_string(),
        column: ColumnDef {
            name: "desc".to_string(),
            col_type: ColType::Text,
            nullable: true,
            primary_key: false,
            unique: false,
            indexed: false,
        },
    }];

    let content = render_migration(&ops, "add_desc_to_posts");
    assert!(content.contains("use sea_orm_migration::prelude::*"));
    assert!(content.contains("DeriveMigrationName"));
    assert!(content.contains("Posts"));
    assert!(content.contains("Desc"));
    assert!(content.contains("add_column"));
    assert!(content.contains("text()"));
    assert!(content.contains("null()"));
    assert!(content.contains("drop_column"));
}

#[test]
fn test_render_create_table() {
    let ops = vec![Operation::CreateTable {
        table: "widgets".to_string(),
        columns: vec![
            ColumnDef { name: "id".to_string(), col_type: ColType::BigInteger, nullable: false, primary_key: true, unique: false, indexed: false },
            ColumnDef { name: "name".to_string(), col_type: ColType::String, nullable: false, primary_key: false, unique: false, indexed: false },
        ],
    }];

    let content = render_migration(&ops, "create_widgets");
    assert!(content.contains("create_table"));
    assert!(content.contains("Widgets"));
    assert!(content.contains("big_integer()"));
    assert!(content.contains("primary_key()"));
    assert!(content.contains("drop_table"));
}

#[test]
fn test_update_lib_rs_appends_mod_and_box() {
    let dir = TempDir::new().unwrap();
    let lib_path = dir.path().join("lib.rs");

    fs::write(&lib_path, r#"pub use sea_orm_migration::prelude::*;

mod m20260324_000001_create_users;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260324_000001_create_users::Migration),
        ]
    }
}
"#).unwrap();

    update_lib_rs(&lib_path, "m20260325_143000_4f2a_add_desc_to_posts").unwrap();

    let updated = fs::read_to_string(&lib_path).unwrap();
    assert!(updated.contains("mod m20260325_143000_4f2a_add_desc_to_posts;"));
    assert!(updated.contains("Box::new(m20260325_143000_4f2a_add_desc_to_posts::Migration)"));
}

#[test]
fn test_update_lib_rs_no_existing_mods() {
    let dir = TempDir::new().unwrap();
    let lib_path = dir.path().join("lib.rs");

    fs::write(&lib_path, r#"pub use sea_orm_migration::prelude::*;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
        ]
    }
}
"#).unwrap();

    update_lib_rs(&lib_path, "m20260325_000001_create_users").unwrap();

    let updated = fs::read_to_string(&lib_path).unwrap();
    assert!(updated.contains("mod m20260325_000001_create_users;"));
    assert!(updated.contains("Box::new(m20260325_000001_create_users::Migration)"));
}
#[test]
fn test_render_alter_column() {
    let ops = vec![Operation::AlterColumn {
        table: "posts".to_string(),
        column: "body".to_string(),
        nullable: true,
    }];

    let content = render_migration(&ops, "alter_posts_body_nullable");
    assert!(content.contains("get_connection()"));
    assert!(content.contains("execute_unprepared"));
    // The generated Rust source code contains \" as escape sequences
    // When the test searches for them in the string, we need to include the backslash
    let expected_up = "ALTER TABLE \\\"posts\\\" ALTER COLUMN \\\"body\\\" DROP NOT NULL";
    assert!(content.contains(expected_up), "Expected to find: {}", expected_up);
    // down() should produce the inverse: SET NOT NULL
    let expected_down = "ALTER TABLE \\\"posts\\\" ALTER COLUMN \\\"body\\\" SET NOT NULL";
    assert!(content.contains(expected_down), "Expected to find: {}", expected_down);
    // No Iden enum for AlterColumn
    assert!(!content.contains("enum Posts"));
}

#[test]
fn test_update_lib_rs_rejects_duplicate() {
    let dir = TempDir::new().unwrap();
    let lib_path = dir.path().join("lib.rs");

    fs::write(&lib_path, r#"pub use sea_orm_migration::prelude::*;

mod m20260324_000001_create_users;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20260324_000001_create_users::Migration),
        ]
    }
}
"#).unwrap();

    let result = update_lib_rs(&lib_path, "m20260324_000001_create_users");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("already registered"));
}

#[test]
fn test_render_add_foreign_key() {
    use seaorm_auto_migration::types::ForeignKeyDef;

    let ops = vec![Operation::AddForeignKey {
        table: "posts".to_string(),
        fk: ForeignKeyDef {
            name: "fk_posts_user_id".to_string(),
            from_col: "user_id".to_string(),
            to_table: "users".to_string(),
            to_col: "id".to_string(),
        },
    }];

    let content = render_migration(&ops, "add_fk_posts_user_id");
    // Writer generates string content — no runtime SeaORM API calls
    assert!(content.contains("create_foreign_key"));
    assert!(content.contains("fk_posts_user_id"));
    assert!(content.contains("Posts::Table"));
    assert!(content.contains("Posts::UserId"));
    assert!(content.contains("Users::Table"));
    assert!(content.contains("Users::Id"));
    // down() should drop the FK
    assert!(content.contains("drop_foreign_key"));
}

#[test]
fn test_render_create_unique_index() {
    use seaorm_auto_migration::types::IndexDef;

    let ops = vec![Operation::CreateIndex {
        table: "users".to_string(),
        index: IndexDef {
            name: "idx_users_email_unique".to_string(),
            columns: vec!["email".to_string()],
            unique: true,
        },
    }];

    let content = render_migration(&ops, "add_idx_users_email");
    assert!(content.contains("create_index"));
    assert!(content.contains("idx_users_email_unique"));
    assert!(content.contains("Users::Table"));
    assert!(content.contains("Users::Email"));
    assert!(content.contains(".unique()"));
    // down() should drop the index
    assert!(content.contains("drop_index"));
}

#[test]
fn test_render_create_non_unique_index() {
    use seaorm_auto_migration::types::IndexDef;

    let ops = vec![Operation::CreateIndex {
        table: "posts".to_string(),
        index: IndexDef {
            name: "idx_posts_title".to_string(),
            columns: vec!["title".to_string()],
            unique: false,
        },
    }];

    let content = render_migration(&ops, "add_idx_posts_title");
    assert!(content.contains("create_index"));
    assert!(content.contains("idx_posts_title"));
    assert!(content.contains("Posts::Table"));
    assert!(content.contains("Posts::Title"));
    // Non-unique: should NOT contain .unique()
    assert!(!content.contains(".unique()"));
    assert!(content.contains("drop_index"));
}

#[test]
fn test_render_alter_column_type() {
    let ops = vec![Operation::AlterColumnType {
        table: "users".to_string(),
        column: "score".to_string(),
        from: ColType::Integer,
        to: ColType::BigInteger,
    }];

    let content = render_migration(&ops, "alter_users_score_type");
    // up() should change to the new type
    assert!(content.contains("execute_unprepared"));
    assert!(content.contains("ALTER TABLE"));
    assert!(content.contains("score"));
    assert!(content.contains("BIGINT"));
    assert!(content.contains("USING"));
    // Should include warning comment
    assert!(content.contains("WARNING"));
    // down() should revert to the old type
    assert!(content.contains("INTEGER"));
}

#[test]
fn test_writer_multi_column_index() {
    use seaorm_auto_migration::types::IndexDef;

    let ops = vec![Operation::CreateIndex {
        table: "posts".to_string(),
        index: IndexDef {
            name: "idx_posts_title_body".to_string(),
            columns: vec!["title".to_string(), "body".to_string()],
            unique: false,
        },
    }];
    let output = render_migration(&ops, "add_multi_col_idx");
    assert!(output.contains(".col(Posts::Title)"), "missing .col(Posts::Title)");
    assert!(output.contains(".col(Posts::Body)"), "missing .col(Posts::Body)");
    assert!(!output.contains(".unique()"), "should not contain .unique()");
    assert!(output.contains("Title"), "Iden enum should contain Title variant");
    assert!(output.contains("Body"), "Iden enum should contain Body variant");
}

#[test]
fn test_writer_multi_column_unique_index() {
    use seaorm_auto_migration::types::IndexDef;

    let ops = vec![Operation::CreateIndex {
        table: "posts".to_string(),
        index: IndexDef {
            name: "idx_posts_a_b_unique".to_string(),
            columns: vec!["a".to_string(), "b".to_string()],
            unique: true,
        },
    }];
    let output = render_migration(&ops, "add_multi_col_unique_idx");
    assert!(output.contains(".col(Posts::A)"), "missing .col(Posts::A)");
    assert!(output.contains(".col(Posts::B)"), "missing .col(Posts::B)");
    assert!(output.contains(".unique()"), "should contain .unique()");
}

#[test]
fn test_writer_single_column_index_still_works() {
    use seaorm_auto_migration::types::IndexDef;

    let ops = vec![Operation::CreateIndex {
        table: "users".to_string(),
        index: IndexDef {
            name: "idx_users_email_unique".to_string(),
            columns: vec!["email".to_string()],
            unique: true,
        },
    }];
    let output = render_migration(&ops, "add_email_idx");
    assert!(output.contains(".col(Users::Email)"), "missing .col(Users::Email)");
    assert!(output.contains(".unique()"), "should contain .unique()");
    assert!(output.contains("Email"), "Iden enum should contain Email variant");
}

#[test]
fn test_writer_drop_multi_column_index_down_recreates_index() {
    use seaorm_auto_migration::types::IndexDef;

    let ops = vec![Operation::DropIndex {
        table: "posts".to_string(),
        index: IndexDef {
            name: "idx_posts_title_body".to_string(),
            columns: vec!["title".to_string(), "body".to_string()],
            unique: false,
        },
    }];
    let output = render_migration(&ops, "drop_multi_col_idx");
    // render_down for DropIndex should recreate the index (using CreateIndex logic)
    assert!(output.contains(".col(Posts::Title)"), "down migration should contain .col(Posts::Title)");
    assert!(output.contains(".col(Posts::Body)"), "down migration should contain .col(Posts::Body)");
}

#[test]
fn test_writer_rename_column_up() {
    let ops = vec![Operation::RenameColumn {
        table: "posts".to_string(),
        from_name: "old_name".to_string(),
        to_name: "new_name".to_string(),
    }];
    let output = render_migration(&ops, "rename_col");
    // render_up produces: .table(Posts::Table).rename_column(Posts::OldName, Posts::NewName)
    assert!(output.contains(".rename_column(Posts::OldName, Posts::NewName)"),
        "up migration should contain rename_column call\nGot:\n{}", output);
    assert!(output.contains("OldName"), "Iden enum should contain OldName variant");
    assert!(output.contains("NewName"), "Iden enum should contain NewName variant");
}

#[test]
fn test_writer_rename_column_down() {
    let ops = vec![Operation::RenameColumn {
        table: "posts".to_string(),
        from_name: "old_name".to_string(),
        to_name: "new_name".to_string(),
    }];
    let output = render_migration(&ops, "rename_col");
    // render_down swaps from_name/to_name
    assert!(output.contains(".rename_column(Posts::NewName, Posts::OldName)"),
        "down migration should swap names\nGot:\n{}", output);
}
