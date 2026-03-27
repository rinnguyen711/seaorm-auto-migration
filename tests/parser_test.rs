use seaorm_auto_migration::parser::{parse_entities, parse_entities_from_str};
use seaorm_auto_migration::types::ColType;
use std::fs;
use tempfile::TempDir;

fn write_entity(dir: &TempDir, filename: &str, content: &str) {
    fs::write(dir.path().join(filename), content).unwrap();
}

#[test]
fn test_parse_simple_entity() {
    let dir = TempDir::new().unwrap();
    write_entity(&dir, "users.rs", r#"
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
    pub email: String,
}
    "#);

    let schemas = parse_entities(dir.path()).unwrap();
    assert_eq!(schemas.len(), 1);
    assert_eq!(schemas[0].table, "users");
    assert_eq!(schemas[0].columns.len(), 3);

    let id_col = schemas[0].columns.iter().find(|c| c.name == "id").unwrap();
    assert!(id_col.primary_key);
    assert!(!id_col.nullable);
    assert_eq!(id_col.col_type, ColType::BigInteger);

    let name_col = schemas[0].columns.iter().find(|c| c.name == "name").unwrap();
    assert!(!name_col.primary_key);
    assert!(!name_col.nullable);
    assert_eq!(name_col.col_type, ColType::String);
}

#[test]
fn test_parse_optional_field_is_nullable() {
    let dir = TempDir::new().unwrap();
    write_entity(&dir, "posts.rs", r#"
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, DeriveEntityModel)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub body: Option<String>,
}
    "#);

    let schemas = parse_entities(dir.path()).unwrap();
    let body_col = schemas[0].columns.iter().find(|c| c.name == "body").unwrap();
    assert!(body_col.nullable);
    assert_eq!(body_col.col_type, ColType::String);
}

#[test]
fn test_skips_non_entity_files() {
    let dir = TempDir::new().unwrap();
    write_entity(&dir, "mod.rs", r#"
pub mod users;
pub mod posts;
    "#);

    let schemas = parse_entities(dir.path()).unwrap();
    assert!(schemas.is_empty());
}

#[test]
fn test_skips_unknown_field_types() {
    let dir = TempDir::new().unwrap();
    write_entity(&dir, "items.rs", r#"
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, DeriveEntityModel)]
#[sea_orm(table_name = "items")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub data: Vec<u8>,
    pub name: String,
}
    "#);

    let schemas = parse_entities(dir.path()).unwrap();
    // Vec<u8> is skipped, only id and name remain
    assert_eq!(schemas[0].columns.len(), 2);
}

#[test]
fn test_parse_entity_with_schema_name() {
    let dir = TempDir::new().unwrap();
    write_entity(&dir, "orders.rs", r#"
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, DeriveEntityModel)]
#[sea_orm(schema_name = "public", table_name = "orders")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub total: i32,
}
    "#);

    let schemas = parse_entities(dir.path()).unwrap();
    assert_eq!(schemas.len(), 1);
    assert_eq!(schemas[0].table, "orders");
    assert_eq!(schemas[0].columns.len(), 2);
}

#[test]
fn test_parse_belongs_to_relation() {
    let dir = TempDir::new().unwrap();
    let entity_path = dir.path().join("posts.rs");
    fs::write(&entity_path, r#"
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub user_id: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::users::Entity",
        from = "Column::UserId",
        to = "super::users::Column::Id"
    )]
    User,
}
"#).unwrap();

    let schemas = parse_entities(dir.path()).unwrap();
    assert_eq!(schemas.len(), 1);
    let fks = &schemas[0].foreign_keys;
    assert_eq!(fks.len(), 1);
    assert_eq!(fks[0].name, "fk_posts_user_id");
    assert_eq!(fks[0].from_col, "user_id");
    assert_eq!(fks[0].to_table, "users");
    assert_eq!(fks[0].to_col, "id");
}

#[test]
fn test_parse_unique_field_generates_index() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("users.rs");
    fs::write(&path, r#"
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(unique)]
    pub email: String,
}
"#).unwrap();

    let schemas = parse_entities(dir.path()).unwrap();
    assert_eq!(schemas.len(), 1);
    let indexes = &schemas[0].indexes;
    assert_eq!(indexes.len(), 1);
    assert_eq!(indexes[0].name, "idx_users_email_unique");
    assert_eq!(indexes[0].columns[0], "email");
    assert!(indexes[0].unique);
}

#[test]
fn test_parse_indexed_field_generates_non_unique_index() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("posts.rs");
    fs::write(&path, r#"
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    #[sea_orm(indexed)]
    pub title: String,
}
"#).unwrap();

    let schemas = parse_entities(dir.path()).unwrap();
    assert_eq!(schemas.len(), 1);
    let indexes = &schemas[0].indexes;
    assert_eq!(indexes.len(), 1);
    assert_eq!(indexes[0].name, "idx_posts_title");
    assert_eq!(indexes[0].columns[0], "title");
    assert!(!indexes[0].unique);
}

#[test]
fn test_primary_key_does_not_generate_index() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("items.rs");
    fs::write(&path, r#"
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "items")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
    pub name: String,
}
"#).unwrap();

    let schemas = parse_entities(dir.path()).unwrap();
    assert_eq!(schemas.len(), 1);
    assert!(schemas[0].indexes.is_empty(), "PK should not produce an IndexDef");
}

#[test]
fn test_has_many_relation_produces_no_fk() {
    let dir = TempDir::new().unwrap();
    let entity_path = dir.path().join("users.rs");
    fs::write(&entity_path, r#"
use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::posts::Entity")]
    Posts,
}
"#).unwrap();

    let schemas = parse_entities(dir.path()).unwrap();
    assert_eq!(schemas.len(), 1);
    assert!(schemas[0].foreign_keys.is_empty(), "has_many should not produce a FK");
}

#[test]
fn test_parser_struct_level_index() {
    let src = r#"
        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "posts", indexes(index(columns = ["title", "body"])))]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub title: String,
            pub body: String,
        }
    "#;
    let schemas = parse_entities_from_str(src);
    assert_eq!(schemas.len(), 1);
    let schema = &schemas[0];
    assert_eq!(schema.indexes.len(), 1);
    let idx = &schema.indexes[0];
    assert_eq!(idx.columns, vec!["title", "body"]);
    assert_eq!(idx.unique, false);
    assert_eq!(idx.name, "idx_posts_title_body");
}

#[test]
fn test_parser_struct_level_unique_index() {
    let src = r#"
        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "posts", indexes(index(columns = ["a", "b"], unique = true)))]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            pub a: String,
            pub b: String,
        }
    "#;
    let schemas = parse_entities_from_str(src);
    assert_eq!(schemas.len(), 1);
    let schema = &schemas[0];
    assert_eq!(schema.indexes.len(), 1);
    let idx = &schema.indexes[0];
    assert_eq!(idx.columns, vec!["a", "b"]);
    assert_eq!(idx.unique, true);
    assert_eq!(idx.name, "idx_posts_a_b_unique");
}

#[test]
fn test_parser_field_level_index_still_works() {
    let src = r#"
        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "users")]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            #[sea_orm(unique)]
            pub email: String,
        }
    "#;
    let schemas = parse_entities_from_str(src);
    assert_eq!(schemas.len(), 1);
    let schema = &schemas[0];
    assert_eq!(schema.indexes.len(), 1);
    let idx = &schema.indexes[0];
    assert_eq!(idx.columns, vec!["email"]);
    assert_eq!(idx.unique, true);
}

#[test]
fn test_parser_overlap_detection_skips_duplicate() {
    // struct-level single-column index on "email" where email already has #[sea_orm(unique)]
    // the struct-level index should be skipped
    let src = r#"
        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "users", indexes(index(columns = ["email"])))]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            #[sea_orm(unique)]
            pub email: String,
        }
    "#;
    let schemas = parse_entities_from_str(src);
    assert_eq!(schemas.len(), 1);
    let schema = &schemas[0];
    // Only 1 index (the field-level unique), not 2
    assert_eq!(schema.indexes.len(), 1);
    assert_eq!(schema.indexes[0].columns, vec!["email"]);
}

#[test]
fn test_parser_combined_field_and_struct_indexes() {
    let src = r#"
        #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
        #[sea_orm(table_name = "users", indexes(index(columns = ["email", "name"])))]
        pub struct Model {
            #[sea_orm(primary_key)]
            pub id: i64,
            #[sea_orm(unique)]
            pub email: String,
            pub name: String,
        }
    "#;
    let schemas = parse_entities_from_str(src);
    assert_eq!(schemas.len(), 1);
    let schema = &schemas[0];
    // Both field-level (email unique) and struct-level (email, name) indexes present
    assert_eq!(schema.indexes.len(), 2);
    let single = schema.indexes.iter().find(|i| i.columns == vec!["email"]).unwrap();
    assert_eq!(single.unique, true);
    let multi = schema.indexes.iter().find(|i| i.columns == vec!["email", "name"]).unwrap();
    assert_eq!(multi.unique, false);
    assert_eq!(multi.name, "idx_users_email_name");
}
