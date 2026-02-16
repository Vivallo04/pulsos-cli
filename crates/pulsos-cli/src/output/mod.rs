pub mod compact;
pub mod json;
pub mod table;

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Compact,
}
