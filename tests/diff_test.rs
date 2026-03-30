use seaorm_auto_migration::diff::compute_diff;
use seaorm_auto_migration::types::*;

fn col(name: &str, col_type: ColType, nullable: bool, primary_key: bool) -> ColumnDef {
    ColumnDef { name: name.to_string(), col_type, nullable, primary_key, unique: false, indexed: false, default_value: None }
}

fn entity(table: &str, columns: Vec<ColumnDef>) -> EntitySchema {
    EntitySchema { table: table.to_string(), columns, foreign_keys: vec![], indexes: vec![] }
}

fn db_table(table: &str, columns: Vec<ColumnDef>) -> TableSchema {
    TableSchema { table: table.to_string(), columns, foreign_keys: vec![], indexes: vec![] }
}

#[test]
fn test_foreign_key_def_exists() {
    let fk = ForeignKeyDef {
        name: "fk_posts_user_id".to_string(),
        from_col: "user_id".to_string(),
        to_table: "users".to_string(),
        to_col: "id".to_string(),
    };
    assert_eq!(fk.name, "fk_posts_user_id");
}

#[test]
fn test_new_entity_creates_table() {
    let entities = vec![
        entity("widgets", vec![
            col("id", ColType::BigInteger, false, true),
            col("name", ColType::String, false, false),
        ])
    ];
    let db = vec![];

    let result = compute_diff(&entities, &db, false, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0], Operation::CreateTable { table, .. } if table == "widgets"));
}

#[test]
fn test_new_field_adds_column() {
    let entities = vec![
        entity("posts", vec![
            col("id", ColType::BigInteger, false, true),
            col("title", ColType::String, false, false),
            col("desc", ColType::Text, true, false),  // new field
        ])
    ];
    let db = vec![
        db_table("posts", vec![
            col("id", ColType::BigInteger, false, true),
            col("title", ColType::String, false, false),
        ])
    ];

    let result = compute_diff(&entities, &db, false, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0], Operation::AddColumn { table, column } if table == "posts" && column.name == "desc"));
}

#[test]
fn test_removed_field_drops_column_with_allow_destructive() {
    let entities = vec![
        entity("posts", vec![
            col("id", ColType::BigInteger, false, true),
        ])
    ];
    let db = vec![
        db_table("posts", vec![
            col("id", ColType::BigInteger, false, true),
            col("old_field", ColType::String, false, false),
        ])
    ];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0], Operation::DropColumn { table, .. } if table == "posts"));
}

#[test]
fn test_removed_field_skipped_without_allow_destructive() {
    let entities = vec![
        entity("posts", vec![
            col("id", ColType::BigInteger, false, true),
        ])
    ];
    let db = vec![
        db_table("posts", vec![
            col("id", ColType::BigInteger, false, true),
            col("old_field", ColType::String, false, false),
        ])
    ];

    let result = compute_diff(&entities, &db, false, |_, _, _| false);
    assert!(result.ops.is_empty());
    assert_eq!(result.destructive_skipped, 1);
}

#[test]
fn test_no_diff_returns_empty() {
    let entities = vec![
        entity("users", vec![
            col("id", ColType::BigInteger, false, true),
            col("name", ColType::String, false, false),
        ])
    ];
    let db = vec![
        db_table("users", vec![
            col("id", ColType::BigInteger, false, true),
            col("name", ColType::String, false, false),
        ])
    ];

    let result = compute_diff(&entities, &db, false, |_, _, _| false);
    assert!(result.ops.is_empty());
    assert_eq!(result.destructive_skipped, 0);
}

#[test]
fn test_orphan_table_dropped_with_allow_destructive() {
    let entities = vec![];
    let db = vec![
        db_table("old_table", vec![
            col("id", ColType::BigInteger, false, true),
        ])
    ];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0], Operation::DropTable { table, .. } if table == "old_table"));
}

#[test]
fn test_type_mismatch_generates_alter_column_type() {
    // type mismatch: entity has String, DB has Integer — should produce AlterColumnType
    let entities = vec![
        entity("items", vec![
            col("id", ColType::BigInteger, false, true),
            col("code", ColType::String, false, false),
        ])
    ];
    let db = vec![
        db_table("items", vec![
            col("id", ColType::BigInteger, false, true),
            col("code", ColType::Integer, false, false),
        ])
    ];

    let result = compute_diff(&entities, &db, false, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0],
        Operation::AlterColumnType { table, column, from, to }
        if table == "items" && column == "code"
        && *from == ColType::Integer && *to == ColType::String
    ));
}

#[test]
fn test_orphan_table_skipped_without_allow_destructive() {
    let entities = vec![];
    let db = vec![
        db_table("old_table", vec![
            col("id", ColType::BigInteger, false, true),
        ])
    ];

    let result = compute_diff(&entities, &db, false, |_, _, _| false);
    assert!(result.ops.is_empty());
    assert_eq!(result.destructive_skipped, 1);
}

#[test]
fn test_nullable_mismatch_generates_alter_column() {
    // nullable mismatch: entity has NOT NULL, DB has nullable → AlterColumn op
    let entities = vec![
        entity("users", vec![
            col("id", ColType::BigInteger, false, true),
            col("email", ColType::String, false, false),  // NOT NULL in entity
        ])
    ];
    let db = vec![
        db_table("users", vec![
            col("id", ColType::BigInteger, false, true),
            col("email", ColType::String, true, false),   // nullable in DB
        ])
    ];

    let result = compute_diff(&entities, &db, false, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(
        &result.ops[0],
        Operation::AlterColumn { table, column, nullable }
            if table == "users" && column == "email" && !nullable
    ));
    assert_eq!(result.destructive_skipped, 0);
}

fn entity_with_fks(table: &str, columns: Vec<ColumnDef>, fks: Vec<ForeignKeyDef>) -> EntitySchema {
    EntitySchema { table: table.to_string(), columns, foreign_keys: fks, indexes: vec![] }
}

fn db_table_with_fks(table: &str, columns: Vec<ColumnDef>, fks: Vec<ForeignKeyDef>) -> TableSchema {
    TableSchema { table: table.to_string(), columns, foreign_keys: fks, indexes: vec![] }
}

fn fk(name: &str, from_col: &str, to_table: &str, to_col: &str) -> ForeignKeyDef {
    ForeignKeyDef {
        name: name.to_string(),
        from_col: from_col.to_string(),
        to_table: to_table.to_string(),
        to_col: to_col.to_string(),
    }
}

#[test]
fn test_new_fk_generates_add_foreign_key() {
    let entities = vec![entity_with_fks(
        "posts",
        vec![col("id", ColType::BigInteger, false, true), col("user_id", ColType::BigInteger, false, false)],
        vec![fk("fk_posts_user_id", "user_id", "users", "id")],
    )];
    let db = vec![db_table_with_fks(
        "posts",
        vec![col("id", ColType::BigInteger, false, true), col("user_id", ColType::BigInteger, false, false)],
        vec![],
    )];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0],
        Operation::AddForeignKey { table, fk } if table == "posts" && fk.name == "fk_posts_user_id"
    ));
}

#[test]
fn test_existing_fk_no_op() {
    let fk_def = fk("fk_posts_user_id", "user_id", "users", "id");
    let entities = vec![entity_with_fks(
        "posts",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![fk_def.clone()],
    )];
    let db = vec![db_table_with_fks(
        "posts",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![fk_def],
    )];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert!(result.ops.is_empty());
}

#[test]
fn test_orphan_fk_generates_drop_foreign_key_when_destructive() {
    let entities = vec![entity_with_fks(
        "posts",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![],
    )];
    let db = vec![db_table_with_fks(
        "posts",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![fk("fk_posts_user_id", "user_id", "users", "id")],
    )];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0],
        Operation::DropForeignKey { table, .. } if table == "posts"
    ));
}

#[test]
fn test_orphan_fk_skipped_without_destructive() {
    let entities = vec![entity_with_fks(
        "posts",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![],
    )];
    let db = vec![db_table_with_fks(
        "posts",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![fk("fk_posts_user_id", "user_id", "users", "id")],
    )];

    let result = compute_diff(&entities, &db, false, |_, _, _| false);
    assert!(result.ops.is_empty());
    assert_eq!(result.destructive_skipped, 1);
}

#[test]
fn test_index_def_exists() {
    let idx = IndexDef {
        name: "idx_users_email_unique".to_string(),
        columns: vec!["email".to_string()],
        unique: true,
    };
    assert_eq!(idx.name, "idx_users_email_unique");
}

fn entity_with_indexes(table: &str, columns: Vec<ColumnDef>, indexes: Vec<IndexDef>) -> EntitySchema {
    EntitySchema { table: table.to_string(), columns, foreign_keys: vec![], indexes }
}

fn db_table_with_indexes(table: &str, columns: Vec<ColumnDef>, indexes: Vec<IndexDef>) -> TableSchema {
    TableSchema { table: table.to_string(), columns, foreign_keys: vec![], indexes }
}

fn idx(name: &str, columns: &[&str], unique: bool) -> IndexDef {
    IndexDef { name: name.to_string(), columns: columns.iter().map(|s| s.to_string()).collect(), unique }
}

#[test]
fn test_new_index_generates_create_index() {
    let entities = vec![entity_with_indexes(
        "users",
        vec![col("id", ColType::BigInteger, false, true), col("email", ColType::String, false, false)],
        vec![idx("idx_users_email_unique", &["email"], true)],
    )];
    let db = vec![db_table_with_indexes(
        "users",
        vec![col("id", ColType::BigInteger, false, true), col("email", ColType::String, false, false)],
        vec![],
    )];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0],
        Operation::CreateIndex { table, index } if table == "users" && index.name == "idx_users_email_unique"
    ));
}

#[test]
fn test_existing_index_no_op() {
    let idx_def = idx("idx_users_email_unique", &["email"], true);
    let entities = vec![entity_with_indexes(
        "users",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![idx_def.clone()],
    )];
    let db = vec![db_table_with_indexes(
        "users",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![idx_def],
    )];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert!(result.ops.is_empty());
}

#[test]
fn test_orphan_index_generates_drop_index_when_destructive() {
    let entities = vec![entity_with_indexes(
        "users",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![],
    )];
    let db = vec![db_table_with_indexes(
        "users",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![idx("idx_users_email_unique", &["email"], true)],
    )];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0],
        Operation::DropIndex { table, .. } if table == "users"
    ));
}

#[test]
fn test_orphan_index_skipped_without_destructive() {
    let entities = vec![entity_with_indexes(
        "users",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![],
    )];
    let db = vec![db_table_with_indexes(
        "users",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![idx("idx_users_email_unique", &["email"], true)],
    )];

    let result = compute_diff(&entities, &db, false, |_, _, _| false);
    assert!(result.ops.is_empty());
    assert_eq!(result.destructive_skipped, 1);
}

#[test]
fn test_fk_with_different_name_same_columns_is_no_op() {
    // DB has "posts_user_id_fkey" (Postgres default), entity has "fk_posts_user_id" (convention).
    // Same (from_col, to_table, to_col) → should produce no operations.
    let entities = vec![entity_with_fks(
        "posts",
        vec![col("id", ColType::BigInteger, false, true), col("user_id", ColType::BigInteger, false, false)],
        vec![fk("fk_posts_user_id", "user_id", "users", "id")],
    )];
    let db = vec![db_table_with_fks(
        "posts",
        vec![col("id", ColType::BigInteger, false, true), col("user_id", ColType::BigInteger, false, false)],
        vec![fk("posts_user_id_fkey", "user_id", "users", "id")],
    )];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert!(result.ops.is_empty(), "Expected no ops, got: {:?}", result.ops);
}

#[test]
fn test_unique_constraint_same_column_is_no_op() {
    // DB has unique constraint "users_email_key" (Postgres default name), entity has
    // #[sea_orm(unique)] which produces index name "idx_users_email_unique".
    // Same (column, unique=true) → should produce no operations.
    let entities = vec![entity_with_indexes(
        "users",
        vec![col("id", ColType::BigInteger, false, true), col("email", ColType::String, false, false)],
        vec![idx("idx_users_email_unique", &["email"], true)],
    )];
    let db = vec![db_table_with_indexes(
        "users",
        vec![col("id", ColType::BigInteger, false, true), col("email", ColType::String, false, false)],
        vec![idx("users_email_key", &["email"], true)],
    )];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert!(result.ops.is_empty(), "Expected no ops, got: {:?}", result.ops);
}

#[test]
fn test_multi_column_index_creates_index() {
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![idx("idx_posts_title_body", &["title", "body"], false)],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], false, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0], Operation::CreateIndex { index, .. } if index.columns == vec!["title", "body"]));
}

#[test]
fn test_multi_column_index_no_op() {
    let cols = vec!["title".to_string(), "body".to_string()];
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![idx("idx_posts_title_body", &["title", "body"], false)],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![IndexDef { name: "idx_posts_title_body".to_string(), columns: cols, unique: false }],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], false, |_, _, _| false);
    assert_eq!(result.ops.len(), 0);
}

#[test]
fn test_multi_column_index_different_order_generates_ops() {
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![idx("idx_posts_a_b", &["a", "b"], false)],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![IndexDef { name: "idx_posts_b_a".to_string(), columns: vec!["b".to_string(), "a".to_string()], unique: false }],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], true, |_, _, _| false);
    assert_eq!(result.ops.len(), 2);
    assert!(result.ops.iter().any(|op| matches!(op, Operation::CreateIndex { .. })));
    assert!(result.ops.iter().any(|op| matches!(op, Operation::DropIndex { .. })));
}

#[test]
fn test_multi_column_unique_index_creates_index() {
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![idx("idx_posts_user_id_created_at_unique", &["user_id", "created_at"], true)],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], false, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0], Operation::CreateIndex { index, .. } if index.unique));
}

#[test]
fn test_orphan_multi_column_index_dropped_with_destructive() {
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![IndexDef { name: "idx_posts_title_body".to_string(), columns: vec!["title".to_string(), "body".to_string()], unique: false }],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], true, |_, _, _| false);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0], Operation::DropIndex { .. }));
}

#[test]
fn test_orphan_multi_column_index_skipped_without_destructive() {
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![],
        indexes: vec![IndexDef { name: "idx_posts_title_body".to_string(), columns: vec!["title".to_string(), "body".to_string()], unique: false }],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], false, |_, _, _| false);
    assert_eq!(result.ops.len(), 0);
    assert_eq!(result.destructive_skipped, 1);
}

// ── Rename column tests ──────────────────────────────────────────────────

#[test]
fn test_rename_column_detected() {
    // DB has "old_name: String NOT NULL", entity has "new_name: String NOT NULL"
    // callback always confirms → RenameColumn (no DropColumn, no AddColumn)
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![col("new_name", ColType::String, false, false)],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![col("old_name", ColType::String, false, false)],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], true, |_, _, _| true);
    assert_eq!(result.ops.len(), 1);
    assert!(matches!(&result.ops[0],
        Operation::RenameColumn { from_name, to_name, .. }
        if from_name == "old_name" && to_name == "new_name"
    ));
}

#[test]
fn test_rename_column_declined() {
    // Same setup but callback declines → DropColumn + AddColumn (2 ops)
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![col("new_name", ColType::String, false, false)],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![col("old_name", ColType::String, false, false)],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], true, |_, _, _| false);
    assert_eq!(result.ops.len(), 2);
    assert!(result.ops.iter().any(|op| matches!(op, Operation::DropColumn { .. })));
    assert!(result.ops.iter().any(|op| matches!(op, Operation::AddColumn { .. })));
}

#[test]
fn test_rename_column_type_mismatch_no_rename() {
    // Type mismatch → no candidates → callback never called → DropColumn + AddColumn
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![col("count", ColType::BigInteger, false, false)],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![col("title", ColType::String, false, false)],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], true, |_, _, _| panic!("should not be called"));
    assert_eq!(result.ops.len(), 2);
    assert!(result.ops.iter().any(|op| matches!(op, Operation::DropColumn { .. })));
    assert!(result.ops.iter().any(|op| matches!(op, Operation::AddColumn { .. })));
}

#[test]
fn test_rename_column_multiple_candidates_first_confirmed() {
    // One dropped column, two added columns with same type.
    // Callback confirms first offer → RenameColumn + AddColumn for remaining.
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![
            col("foo", ColType::String, false, false),
            col("bar", ColType::String, false, false),
        ],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![col("old_a", ColType::String, false, false)],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], true, |_, _, _| true);
    assert_eq!(result.ops.len(), 2);
    assert!(result.ops.iter().any(|op| matches!(op, Operation::RenameColumn { .. })));
    assert!(result.ops.iter().any(|op| matches!(op, Operation::AddColumn { .. })));
}

#[test]
fn test_rename_column_multiple_candidates_second_confirmed() {
    // Decline first candidate, confirm second.
    use std::cell::Cell;
    let call_count = Cell::new(0usize);
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![
            col("foo", ColType::String, false, false),
            col("bar", ColType::String, false, false),
        ],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![col("old_a", ColType::String, false, false)],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], true, |_, _, _| {
        let n = call_count.get();
        call_count.set(n + 1);
        n == 1 // decline first call (n=0), confirm second call (n=1)
    });
    assert_eq!(call_count.get(), 2);
    assert!(result.ops.iter().any(|op| matches!(op, Operation::RenameColumn { .. })));
    assert!(result.ops.iter().any(|op| matches!(op, Operation::AddColumn { .. })));
}

#[test]
fn test_rename_column_declined_no_destructive() {
    // Callback declines + allow_destructive=false.
    // Rename prompt IS shown (independent of allow_destructive).
    // After decline: DropColumn suppressed → destructive_skipped=1.
    // AddColumn still emitted (adds never suppressed).
    let entity = EntitySchema {
        table: "posts".to_string(),
        columns: vec![col("new_name", ColType::String, false, false)],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let db = TableSchema {
        table: "posts".to_string(),
        columns: vec![col("old_name", ColType::String, false, false)],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let result = compute_diff(&[entity], &[db], false, |_, _, _| false);
    assert_eq!(result.destructive_skipped, 1);
    assert!(!result.ops.iter().any(|op| matches!(op, Operation::DropColumn { .. })));
    assert!(result.ops.iter().any(|op| matches!(op, Operation::AddColumn { .. })));
}

#[test]
fn test_new_table_with_fk_to_existing_table_inlines_fk() {
    // posts is new, users already exists in DB.
    // FK should be inline in CreateTable, not a separate AddForeignKey.
    let entities = vec![
        // users entity already exists in DB
        entity("users", vec![col("id", ColType::BigInteger, false, true)]),
        // posts is new with FK to users
        entity_with_fks(
            "posts",
            vec![
                col("id", ColType::BigInteger, false, true),
                col("user_id", ColType::BigInteger, false, false),
            ],
            vec![fk("fk_posts_user_id", "user_id", "users", "id")],
        ),
    ];
    let db = vec![db_table("users", vec![col("id", ColType::BigInteger, false, true)])];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert_eq!(result.ops.len(), 1, "expected only CreateTable, got: {:?}", result.ops);
    match &result.ops[0] {
        Operation::CreateTable { table, foreign_keys, .. } => {
            assert_eq!(table, "posts");
            assert_eq!(foreign_keys.len(), 1);
            assert_eq!(foreign_keys[0].name, "fk_posts_user_id");
            assert_eq!(foreign_keys[0].from_col, "user_id");
            assert_eq!(foreign_keys[0].to_table, "users");
            assert_eq!(foreign_keys[0].to_col, "id");
        }
        other => panic!("expected CreateTable, got {:?}", other),
    }
}

#[test]
fn test_two_new_tables_sorted_by_fk_dependency() {
    // posts depends on users (FK user_id → users.id).
    // Both are new. users must come first in the output.
    let entities = vec![
        entity_with_fks(
            "posts",
            vec![
                col("id", ColType::BigInteger, false, true),
                col("user_id", ColType::BigInteger, false, false),
            ],
            vec![fk("fk_posts_user_id", "user_id", "users", "id")],
        ),
        entity_with_fks(
            "users",
            vec![col("id", ColType::BigInteger, false, true)],
            vec![],
        ),
    ];
    let db = vec![];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);
    assert_eq!(result.ops.len(), 2);
    let tables: Vec<&str> = result.ops.iter().map(|op| match op {
        Operation::CreateTable { table, .. } => table.as_str(),
        _ => panic!("expected only CreateTable ops"),
    }).collect();
    assert_eq!(tables, vec!["users", "posts"], "users must come before posts");
}

#[test]
fn test_new_table_with_two_fks_to_same_table_not_treated_as_cyclic() {
    // orders has two FKs to customers — should not be treated as cyclic.
    // customers must come before orders.
    let entities = vec![
        entity_with_fks(
            "orders",
            vec![
                col("id", ColType::BigInteger, false, true),
                col("customer_id", ColType::BigInteger, false, false),
                col("billing_customer_id", ColType::BigInteger, false, false),
            ],
            vec![
                fk("fk_orders_customer_id", "customer_id", "customers", "id"),
                fk("fk_orders_billing_customer_id", "billing_customer_id", "customers", "id"),
            ],
        ),
        entity_with_fks(
            "customers",
            vec![col("id", ColType::BigInteger, false, true)],
            vec![],
        ),
    ];
    let db = vec![];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);

    let create_count = result.ops.iter().filter(|op| matches!(op, Operation::CreateTable { .. })).count();
    let fk_count = result.ops.iter().filter(|op| matches!(op, Operation::AddForeignKey { .. })).count();

    assert_eq!(create_count, 2, "both tables should be created");
    assert_eq!(fk_count, 0, "no FKs should be demoted — this is not a cycle");

    let tables: Vec<&str> = result.ops.iter().filter_map(|op| match op {
        Operation::CreateTable { table, .. } => Some(table.as_str()),
        _ => None,
    }).collect();
    assert_eq!(tables, vec!["customers", "orders"], "customers must come before orders");
}

#[test]
fn test_circular_fk_between_new_tables_demotes_one_fk() {
    // a has FK to b, b has FK to a — a cycle.
    // One FK must be demoted to a trailing AddForeignKey.
    let entities = vec![
        entity_with_fks(
            "a",
            vec![col("id", ColType::BigInteger, false, true), col("b_id", ColType::BigInteger, true, false)],
            vec![fk("fk_a_b", "b_id", "b", "id")],
        ),
        entity_with_fks(
            "b",
            vec![col("id", ColType::BigInteger, false, true), col("a_id", ColType::BigInteger, true, false)],
            vec![fk("fk_b_a", "a_id", "a", "id")],
        ),
    ];
    let db = vec![];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);

    let create_count = result.ops.iter().filter(|op| matches!(op, Operation::CreateTable { .. })).count();
    let fk_count = result.ops.iter().filter(|op| matches!(op, Operation::AddForeignKey { .. })).count();

    assert_eq!(create_count, 2, "both tables must be created");
    assert_eq!(fk_count, 1, "one FK must be demoted to AddForeignKey to break the cycle");
}

#[test]
fn test_existing_table_fk_to_new_table_ordering() {
    // posts exists in DB, tags is new.
    // posts gets a new FK to tags in this diff.
    // CreateTable(tags) must come before AddForeignKey(posts→tags).
    let entities = vec![
        entity_with_fks(
            "posts",
            vec![
                col("id", ColType::BigInteger, false, true),
                col("tag_id", ColType::BigInteger, true, false),
            ],
            vec![fk("fk_posts_tag_id", "tag_id", "tags", "id")],
        ),
        entity_with_fks(
            "tags",
            vec![col("id", ColType::BigInteger, false, true)],
            vec![],
        ),
    ];
    let db = vec![db_table_with_fks(
        "posts",
        vec![col("id", ColType::BigInteger, false, true)],
        vec![],
    )];

    let result = compute_diff(&entities, &db, true, |_, _, _| false);

    // Expect: CreateTable(tags), AddColumn(posts.tag_id), AddForeignKey(posts→tags)
    let create_pos = result.ops.iter().position(|op| matches!(op, Operation::CreateTable { table, .. } if table == "tags"));
    let fk_pos = result.ops.iter().position(|op| matches!(op, Operation::AddForeignKey { table, .. } if table == "posts"));

    assert!(create_pos.is_some(), "CreateTable(tags) should be emitted");
    assert!(fk_pos.is_some(), "AddForeignKey(posts→tags) should be emitted");
    assert!(
        create_pos.unwrap() < fk_pos.unwrap(),
        "CreateTable(tags) must come before AddForeignKey(posts→tags)"
    );
}

#[test]
fn test_normalize_default_quoted_string_with_cast() {
    assert_eq!(ColType::normalize_default("'active'::character varying"), "active");
}

#[test]
fn test_normalize_default_quoted_string_with_text_cast() {
    assert_eq!(ColType::normalize_default("'hello world'::text"), "hello world");
}

#[test]
fn test_normalize_default_true_uppercase() {
    assert_eq!(ColType::normalize_default("TRUE"), "true");
}

#[test]
fn test_normalize_default_false_uppercase() {
    assert_eq!(ColType::normalize_default("FALSE"), "false");
}

#[test]
fn test_normalize_default_numeric_integer() {
    assert_eq!(ColType::normalize_default("0"), "0");
}

#[test]
fn test_normalize_default_numeric_decimal() {
    assert_eq!(ColType::normalize_default("3.14"), "3.14");
}

#[test]
fn test_normalize_default_empty_string() {
    assert_eq!(ColType::normalize_default("''::text"), "");
}

#[test]
fn test_normalize_default_url_value_not_truncated() {
    // URL contains '::' inside a quoted string — must NOT be stripped at the first '::'
    assert_eq!(ColType::normalize_default("'http://example.com'::text"), "http://example.com");
}

#[test]
fn test_normalize_default_escaped_single_quote() {
    // Postgres stores single quotes inside strings as ''
    assert_eq!(ColType::normalize_default("'it''s fine'::text"), "it's fine");
}
