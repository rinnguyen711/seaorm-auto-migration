#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDef {
    pub name: String,
    pub col_type: ColType,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub indexed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColType {
    String,
    Text,
    Integer,
    SmallInteger,
    BigInteger,
    Boolean,
    Float,
    Double,
    Decimal,
    DateTime,
    DateTimeWithTz,
    Date,
    Json,
    Uuid,
}

impl ColType {
    pub fn from_rust_type(ty: &str) -> Option<Self> {
        match ty {
            "String" => Some(Self::String),
            "i16" => Some(Self::SmallInteger),
            "i32" => Some(Self::Integer),
            "i64" => Some(Self::BigInteger),
            "bool" => Some(Self::Boolean),
            "f32" => Some(Self::Float),
            "f64" => Some(Self::Double),
            "Decimal" => Some(Self::Decimal),
            "DateTime" | "DateTimeUtc" | "DateTimeLocal" => Some(Self::DateTime),
            "DateTimeWithTimeZone" => Some(Self::DateTimeWithTz),
            "Date" => Some(Self::Date),
            "Json" | "Value" => Some(Self::Json),
            "Uuid" => Some(Self::Uuid),
            _ => None,
        }
    }

    pub fn from_sql_type(ty: &str) -> Option<Self> {
        match ty {
            "character varying" | "character" => Some(Self::String),
            "text" => Some(Self::Text),
            "integer" => Some(Self::Integer),
            "smallint" => Some(Self::SmallInteger),
            "bigint" => Some(Self::BigInteger),
            "boolean" => Some(Self::Boolean),
            "real" => Some(Self::Float),
            "double precision" => Some(Self::Double),
            "numeric" | "decimal" => Some(Self::Decimal),
            "timestamp without time zone" => Some(Self::DateTime),
            "timestamp with time zone" => Some(Self::DateTimeWithTz),
            "date" => Some(Self::Date),
            "json" | "jsonb" => Some(Self::Json),
            "uuid" => Some(Self::Uuid),
            _ => None,
        }
    }

    pub fn to_sql_type(&self) -> &'static str {
        match self {
            Self::String => "VARCHAR",
            Self::Text => "TEXT",
            Self::Integer => "INTEGER",
            Self::SmallInteger => "SMALLINT",
            Self::BigInteger => "BIGINT",
            Self::Boolean => "BOOLEAN",
            Self::Float => "REAL",
            Self::Double => "DOUBLE PRECISION",
            Self::Decimal => "NUMERIC",
            Self::DateTime => "TIMESTAMP WITHOUT TIME ZONE",
            Self::DateTimeWithTz => "TIMESTAMP WITH TIME ZONE",
            Self::Date => "DATE",
            Self::Json => "JSONB",
            Self::Uuid => "UUID",
        }
    }

    pub fn to_seaorm_method(&self) -> &'static str {
        match self {
            Self::String => "string()",
            Self::Text => "text()",
            Self::Integer => "integer()",
            Self::SmallInteger => "small_integer()",
            Self::BigInteger => "big_integer()",
            Self::Boolean => "boolean()",
            Self::Float => "float()",
            Self::Double => "double()",
            Self::Decimal => "decimal()",
            Self::DateTime => "date_time()",
            Self::DateTimeWithTz => "timestamp_with_time_zone()",
            Self::Date => "date()",
            Self::Json => "json()",
            Self::Uuid => "uuid()",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKeyDef {
    /// Constraint name, e.g. "fk_posts_user_id"
    pub name: String,
    /// Column on this table, e.g. "user_id"
    pub from_col: String,
    /// Referenced table, e.g. "users"
    pub to_table: String,
    /// Referenced column, e.g. "id"
    pub to_col: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexDef {
    /// Index name, e.g. "idx_users_email_unique" or "idx_posts_title"
    pub name: String,
    /// Columns the index is on, e.g. ["email"] or ["user_id", "post_id"]
    pub columns: Vec<String>,
    /// Whether this is a unique index
    pub unique: bool,
}

/// Represents a schema parsed from a Rust SeaORM entity definition.
/// This is the "desired" state derived from source code, not the live database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitySchema {
    pub table: String,
    pub columns: Vec<ColumnDef>,
    pub foreign_keys: Vec<ForeignKeyDef>,
    pub indexes: Vec<IndexDef>,
}

/// Represents a schema introspected from a live database table.
/// This is the "current" state of the database, not the Rust entity definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableSchema {
    pub table: String,
    pub columns: Vec<ColumnDef>,
    pub foreign_keys: Vec<ForeignKeyDef>,
    pub indexes: Vec<IndexDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Operation {
    CreateTable {
        table: String,
        columns: Vec<ColumnDef>,
    },
    AddColumn {
        table: String,
        column: ColumnDef,
    },
    DropColumn {
        table: String,
        column: ColumnDef,
    },
    DropTable {
        table: String,
        /// The column list is retained so the `down()` migration can recreate
        /// the table exactly as it was before it was dropped (rollback support).
        columns: Vec<ColumnDef>,
    },
    AlterColumn {
        table: String,
        column: String,
        /// The new nullability (true = nullable, false = NOT NULL).
        nullable: bool,
    },
    AlterColumnType {
        table: String,
        column: String,
        from: ColType,
        to: ColType,
    },
    AddForeignKey {
        table: String,
        fk: ForeignKeyDef,
    },
    DropForeignKey {
        table: String,
        fk: ForeignKeyDef,
    },
    CreateIndex {
        table: String,
        index: IndexDef,
    },
    DropIndex {
        table: String,
        index: IndexDef,
    },
}

impl Operation {
    pub fn is_destructive(&self) -> bool {
        matches!(self,
            Self::DropColumn { .. } | Self::DropTable { .. } | Self::DropForeignKey { .. } | Self::DropIndex { .. }
        )
    }
}
