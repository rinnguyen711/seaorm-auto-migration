use sqlx::PgPool;
use sqlx::Row;
use crate::types::{ColType, ColumnDef, ForeignKeyDef, IndexDef, TableSchema};

pub async fn read_schema(pool: &PgPool) -> anyhow::Result<Vec<TableSchema>> {
    let rows = sqlx::query(
        r#"
        SELECT
            c.table_name,
            c.column_name,
            c.data_type,
            c.is_nullable,
            CASE WHEN kcu.column_name IS NOT NULL THEN true ELSE false END::boolean AS is_primary_key
        FROM information_schema.columns c
        LEFT JOIN information_schema.key_column_usage kcu
            ON kcu.table_schema = 'public'
            AND kcu.table_name = c.table_name
            AND kcu.column_name = c.column_name
            AND kcu.constraint_name IN (
                SELECT constraint_name
                FROM information_schema.table_constraints
                WHERE constraint_type = 'PRIMARY KEY'
                AND table_schema = 'public'
                AND table_name = c.table_name
            )
        WHERE c.table_schema = 'public'
        ORDER BY c.table_name, c.ordinal_position
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut map: std::collections::BTreeMap<String, Vec<ColumnDef>> = std::collections::BTreeMap::new();

    for row in &rows {
        let table_name: String = row.try_get("table_name")?;
        let column_name: String = row.try_get("column_name")?;
        let data_type: String = row.try_get("data_type")?;
        let is_nullable: String = row.try_get("is_nullable")?;
        let is_primary_key: bool = row.try_get("is_primary_key")?;

        let col_type = match ColType::from_sql_type(&data_type) {
            Some(t) => t,
            None => {
                eprintln!(
                    "Warning: unknown SQL type '{}' on '{}.{}' — skipping",
                    data_type, table_name, column_name
                );
                continue;
            }
        };

        let col = ColumnDef {
            name: column_name,
            col_type,
            nullable: is_nullable == "YES",
            primary_key: is_primary_key,
            unique: false,
            indexed: false,
        };

        map.entry(table_name).or_default().push(col);
    }

    let fk_rows = sqlx::query(
        r#"
    SELECT
        tc.table_name AS from_table,
        kcu.column_name AS from_col,
        ccu.table_name AS to_table,
        ccu.column_name AS to_col,
        tc.constraint_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON kcu.constraint_name = tc.constraint_name
        AND kcu.table_schema = tc.table_schema
    JOIN information_schema.referential_constraints rc
        ON rc.constraint_name = tc.constraint_name
        AND rc.constraint_schema = tc.table_schema
    JOIN information_schema.constraint_column_usage ccu
        ON ccu.constraint_name = rc.unique_constraint_name
        AND ccu.table_schema = tc.table_schema
    WHERE tc.constraint_type = 'FOREIGN KEY'
    AND tc.table_schema = 'public'
    ORDER BY tc.table_name, tc.constraint_name
    "#,
    )
    .fetch_all(pool)
    .await?;

    // NOTE: must be `mut` so we can call `.remove()` below
    let mut fk_map: std::collections::BTreeMap<String, Vec<ForeignKeyDef>> =
        std::collections::BTreeMap::new();

    for row in &fk_rows {
        let from_table: String = row.try_get("from_table")?;
        let from_col: String = row.try_get("from_col")?;
        let to_table: String = row.try_get("to_table")?;
        let to_col: String = row.try_get("to_col")?;
        let constraint_name: String = row.try_get("constraint_name")?;

        fk_map.entry(from_table).or_default().push(ForeignKeyDef {
            name: constraint_name,
            from_col,
            to_table,
            to_col,
        });
    }

    let idx_rows = sqlx::query(
        r#"
    SELECT
        t.relname AS table_name,
        i.relname AS index_name,
        a.attname AS column_name,
        ix.indisunique AS is_unique
    FROM pg_class t
    JOIN pg_index ix ON ix.indrelid = t.oid
    JOIN pg_class i ON i.oid = ix.indexrelid
    JOIN pg_attribute a ON a.attrelid = t.oid AND a.attnum = ANY(ix.indkey)
    JOIN pg_namespace n ON n.oid = t.relnamespace
    WHERE n.nspname = 'public'
      AND t.relkind = 'r'
      AND NOT ix.indisprimary
      AND array_length(ix.indkey, 1) = 1
    ORDER BY t.relname, i.relname
    "#,
    )
    .fetch_all(pool)
    .await?;

    // NOTE: must be `mut` so we can call `.remove()` below
    let mut idx_map: std::collections::BTreeMap<String, Vec<IndexDef>> =
        std::collections::BTreeMap::new();

    for row in &idx_rows {
        let table_name: String = row.try_get("table_name")?;
        let index_name: String = row.try_get("index_name")?;
        let column_name: String = row.try_get("column_name")?;
        let is_unique: bool = row.try_get("is_unique")?;

        idx_map.entry(table_name).or_default().push(IndexDef {
            name: index_name,
            columns: vec![column_name],
            unique: is_unique,
        });
    }

    Ok(map
        .into_iter()
        .map(|(table, columns)| {
            let foreign_keys = fk_map.remove(&table).unwrap_or_default();
            let indexes = idx_map.remove(&table).unwrap_or_default();
            TableSchema { table, columns, foreign_keys, indexes }
        })
        .collect())
}

/// Returns the set of migration names already applied, as recorded in `seaql_migrations`.
/// Returns an empty set if the table doesn't exist yet (fresh database).
pub async fn read_applied_migrations(pool: &PgPool) -> anyhow::Result<std::collections::HashSet<String>> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_schema = 'public' AND table_name = 'seaql_migrations')"
    )
    .fetch_one(pool)
    .await?;

    if !exists {
        return Ok(std::collections::HashSet::new());
    }

    let rows = sqlx::query("SELECT version FROM seaql_migrations")
        .fetch_all(pool)
        .await?;

    Ok(rows.iter().map(|r| r.try_get::<String, _>("version").unwrap_or_default()).collect())
}

pub async fn connect(database_url: &str) -> anyhow::Result<PgPool> {
    PgPool::connect(database_url)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to database: {}", e))
}
