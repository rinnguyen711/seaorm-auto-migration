# seaorm-auto-migration

Auto migration generator for [SeaORM](https://www.sea-ql.org/SeaORM/).

Diffs your SeaORM entity files against a live PostgreSQL database and generates ready-to-run migration files — no manual writing required.

## Install

```bash
cargo install seaorm-auto-migration
```

## Usage

```bash
seaorm-auto-migration generate "add desc to posts"
```

With explicit options:

```bash
seaorm-auto-migration generate "add desc to posts" \
  --entities src/entities/ \
  --database-url postgres://user:pass@localhost/mydb \
  --migration-dir migration/src/
```

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--entities` | `src/entities/` | Path to entity `.rs` files |
| `--migration-dir` | `migration/src/` | Where to write migration files |
| `--database-url` | `DATABASE_URL` env | PostgreSQL connection URL |
| `--no-destructive` | false | Skip `DropColumn`, `DropTable`, `DropForeignKey`, `DropIndex`, `DropDefault` generation |

## Multi-column indexes

Declare multi-column indexes at the struct level alongside `table_name`:

```rust
#[sea_orm(
    table_name = "posts",
    indexes(
        index(columns = ["title", "body"]),
        index(columns = ["user_id", "created_at"], unique = true),
    )
)]
pub struct Model { ... }
```

Index names are auto-generated as `idx_{table}_{col1}_{col2}[_unique]`.

## Rename column detection

When a column disappears from an entity and a new column appears on the same table with matching type, nullability, and primary key flag, the tool prompts interactively:

```
Did you rename column "old_name" to "new_name" on table "posts"? [y/N]
```

- `y` → emits `RenameColumn` (non-destructive, reversible)
- `N` or empty → emits `DropColumn + AddColumn` as usual (DropColumn is gated by `--no-destructive`)

Rename + type change simultaneously is not detected as a rename (it appears as drop + add).

## What it detects

| Change | Generated operation |
|--------|-------------------|
| New field in entity | `AddColumn` |
| New entity file | `CreateTable` |
| Field removed from entity | `DropColumn` |
| Entity file removed | `DropTable` |
| Nullability change on a field | `AlterColumn` (SET/DROP NOT NULL) |
| Column type change | `AlterColumnType` (raw SQL with USING cast — review before running) |
| FK in entity (`belongs_to`) not in DB | `AddForeignKey` |
| FK on a newly created table | Inline `ForeignKey` in `CreateTable` (topologically sorted) |
| FK in DB not in entity | `DropForeignKey` |
| Field with `#[sea_orm(unique)]` not indexed in DB | `CreateIndex` (unique) |
| Field with `#[sea_orm(indexed)]` not indexed in DB | `CreateIndex` |
| Multi-column index in entity not in DB | `CreateIndex` |
| Index in DB not in entity | `DropIndex` |
| Column renamed (confirmed interactively) | `RenameColumn` |
| `default_value` added or changed on a field | `SetDefault` |
| `default_value` removed from a field | `DropDefault` (gated by `--no-destructive`) |

## Default values

Declare a column default with `#[sea_orm(default_value = "...")]`:

```rust
#[sea_orm(default_value = "active")]
pub status: String,

#[sea_orm(default_value = "0")]
pub count: i32,
```

- Adding or changing a default emits `SetDefault` (non-destructive).
- Removing a default emits `DropDefault`, which is gated by `--no-destructive`.
- `default_expr` (SQL function defaults like `now()`) is not supported and emits a warning.

## What it does NOT detect

- Partial indexes, expression indexes, non-default index methods (btree assumed)
- Composite (multi-column) foreign keys
- MySQL / SQLite (PostgreSQL only)

## Supported field types

`String`, `i16`, `i32`, `i64`, `bool`, `f32`, `f64`, `Decimal`, `DateTime`, `DateTimeWithTimeZone`, `Date`, `Json`, `Uuid`, `Option<T>` (nullable)

## MSRV

Rust 1.85+
