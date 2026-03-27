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
| `--no-destructive` | false | Skip `DropColumn`, `DropTable`, `DropForeignKey`, `DropIndex` generation |

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
| FK in DB not in entity | `DropForeignKey` |
| Field with `#[sea_orm(unique)]` not indexed in DB | `CreateIndex` (unique) |
| Field with `#[sea_orm(indexed)]` not indexed in DB | `CreateIndex` |
| Multi-column index in entity not in DB | `CreateIndex` |
| Index in DB not in entity | `DropIndex` |

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

## What it does NOT detect

- Renames
- Partial indexes, expression indexes, non-default index methods (btree assumed)
- Composite (multi-column) foreign keys
- FKs on newly created tables (run the tool twice: first to create the table, then to add the FK)
- MySQL / SQLite (PostgreSQL only)

## Supported field types

`String`, `i16`, `i32`, `i64`, `bool`, `f32`, `f64`, `Decimal`, `DateTime`, `DateTimeWithTimeZone`, `Date`, `Json`, `Uuid`, `Option<T>` (nullable)

## MSRV

Rust 1.85+
