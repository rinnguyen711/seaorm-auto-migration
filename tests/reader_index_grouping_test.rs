use seaorm_auto_migration::reader::group_index_rows;

#[test]
fn test_group_single_column_index() {
    // rows: (table_name, index_name, column_name, is_unique)
    let rows = vec![
        ("users".to_string(), "idx_users_email".to_string(), "email".to_string(), false),
    ];
    let result = group_index_rows(rows);
    assert_eq!(result.len(), 1);
    let (table, indexes) = &result[0];
    assert_eq!(table, "users");
    assert_eq!(indexes.len(), 1);
    assert_eq!(indexes[0].name, "idx_users_email");
    assert_eq!(indexes[0].columns, vec!["email"]);
    assert!(!indexes[0].unique);
}

#[test]
fn test_group_multi_column_index() {
    let rows = vec![
        ("posts".to_string(), "idx_posts_title_body".to_string(), "title".to_string(), false),
        ("posts".to_string(), "idx_posts_title_body".to_string(), "body".to_string(), false),
    ];
    let result = group_index_rows(rows);
    assert_eq!(result.len(), 1);
    let (table, indexes) = &result[0];
    assert_eq!(table, "posts");
    assert_eq!(indexes.len(), 1);
    assert_eq!(indexes[0].columns, vec!["title", "body"]);
    assert!(!indexes[0].unique);
}

#[test]
fn test_group_multiple_indexes_same_table() {
    let rows = vec![
        ("users".to_string(), "idx_users_email".to_string(), "email".to_string(), true),
        ("users".to_string(), "idx_users_name_age".to_string(), "name".to_string(), false),
        ("users".to_string(), "idx_users_name_age".to_string(), "age".to_string(), false),
    ];
    let result = group_index_rows(rows);
    assert_eq!(result.len(), 1);
    let (table, indexes) = &result[0];
    assert_eq!(table, "users");
    assert_eq!(indexes.len(), 2);
    // idx_users_email
    assert_eq!(indexes[0].name, "idx_users_email");
    assert_eq!(indexes[0].columns, vec!["email"]);
    assert!(indexes[0].unique);
    // idx_users_name_age
    assert_eq!(indexes[1].name, "idx_users_name_age");
    assert_eq!(indexes[1].columns, vec!["name", "age"]);
    assert!(!indexes[1].unique);
}

#[test]
fn test_group_indexes_multiple_tables() {
    let rows = vec![
        ("posts".to_string(), "idx_posts_a_b".to_string(), "a".to_string(), false),
        ("posts".to_string(), "idx_posts_a_b".to_string(), "b".to_string(), false),
        ("users".to_string(), "idx_users_email".to_string(), "email".to_string(), true),
    ];
    let result = group_index_rows(rows);
    assert_eq!(result.len(), 2);
    // Table order follows BTreeMap (alphabetical): posts, users
    assert_eq!(result[0].0, "posts");
    assert_eq!(result[0].1[0].columns, vec!["a", "b"]);
    assert_eq!(result[1].0, "users");
    assert_eq!(result[1].1[0].columns, vec!["email"]);
}

#[test]
fn test_group_empty_rows() {
    let rows: Vec<(String, String, String, bool)> = vec![];
    let result = group_index_rows(rows);
    assert!(result.is_empty());
}
