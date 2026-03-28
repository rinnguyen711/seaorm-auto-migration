pub mod diff;
pub mod parser;
pub mod reader;
pub mod types;
pub mod writer;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "seaorm-auto-migration", about = "Auto-generate SeaORM migrations by diffing entities against the live DB")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Generate a migration file by diffing entities against the live database
    Generate {
        /// Human-readable migration message (used in filename)
        message: String,

        /// Path to entity .rs files
        #[arg(long, default_value = "src/entities")]
        entities: PathBuf,

        /// PostgreSQL connection URL (overrides DATABASE_URL env var)
        #[arg(long)]
        database_url: Option<String>,

        /// Output directory for migration files
        #[arg(long, default_value = "migration/src")]
        migration_dir: PathBuf,

        /// Skip generating destructive operations (DropColumn, DropTable)
        #[arg(long, default_value_t = false)]
        no_destructive: bool,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Generate {
            message,
            entities,
            database_url,
            migration_dir,
            no_destructive,
        } => {
            // Resolve database URL
            dotenvy::dotenv().ok();
            let db_url = database_url
                .or_else(|| std::env::var("DATABASE_URL").ok())
                .ok_or_else(|| anyhow::anyhow!(
                    "No database URL provided. Use --database-url or set DATABASE_URL"
                ))?;

            // Parse entities
            eprintln!("Parsing entities from {}...", entities.display());
            let entity_schemas = parser::parse_entities(&entities)?;
            if entity_schemas.is_empty() {
                eprintln!("No SeaORM entities found in {}", entities.display());
                return Ok(());
            }

            // Read DB schema
            eprintln!("Reading database schema...");
            let pool = reader::connect(&db_url).await?;

            // Check for unapplied migrations before generating a new one
            let applied = reader::read_applied_migrations(&pool).await?;
            let mut pending: Vec<String> = Vec::new();
            if migration_dir.exists() {
                for entry in std::fs::read_dir(&migration_dir)? {
                    let entry = entry?;
                    let fname = entry.file_name().to_string_lossy().to_string();
                    if fname == "lib.rs" || !fname.ends_with(".rs") { continue; }
                    // Migration files match pattern: mYYYYMMDD_HHMMSS_xxxx_slug.rs
                    if !fname.starts_with('m') || fname.len() < 20 { continue; }
                    let migration_name = fname.trim_end_matches(".rs").to_string();
                    if !applied.contains(&migration_name) {
                        pending.push(migration_name);
                    }
                }
            }
            if !pending.is_empty() {
                pending.sort();
                eprintln!("Error: {} unapplied migration(s) found:", pending.len());
                for name in &pending {
                    eprintln!("  - {}", name);
                }
                eprintln!("\nApply pending migrations before generating a new one:");
                eprintln!("  sea-orm-cli migrate up");
                return Err(anyhow::anyhow!("Unapplied migrations must be applied first"));
            }

            let db_schemas = reader::read_schema(&pool).await?;

            // Compute diff
            let ask_rename = |table: &str, from: &str, to: &str| -> bool {
                eprint!("Did you rename column \"{}\" to \"{}\" on table \"{}\"? [y/N] ", from, to, table);
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).unwrap_or(0);
                input.trim().eq_ignore_ascii_case("y")
            };
            let result = diff::compute_diff(&entity_schemas, &db_schemas, !no_destructive, ask_rename);
            if result.ops.is_empty() {
                if result.destructive_skipped == 0 {
                    println!("No changes detected.");
                } else {
                    println!("No changes detected. {} destructive operation(s) skipped. Re-run without --no-destructive to include them.", result.destructive_skipped);
                }
                return Ok(());
            }

            // Write migration file
            let filename = writer::generate_filename(&message);
            let migration_name = filename.trim_end_matches(".rs").to_string();
            let ops = result.ops;
            let output_path = migration_dir.join(&filename);

            let content = writer::render_migration(&ops, &migration_name);
            std::fs::write(&output_path, &content)?;
            println!("Generated: {}", output_path.display());

            // Update lib.rs
            let lib_path = migration_dir.join("lib.rs");
            if lib_path.exists() {
                writer::update_lib_rs(&lib_path, &migration_name)?;
                println!("Updated: {}", lib_path.display());
            } else {
                eprintln!("Warning: lib.rs not found at {}. Add the migration manually.", lib_path.display());
            }

            Ok(())
        }
    }
}
