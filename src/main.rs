use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

// ============================================================================
// Error Handling
// ============================================================================

#[derive(Debug)]
enum AppError {
    ConfigError(String),
    FileError(String),
    ParseError(String),
    IoError(String),
    ValidationError(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::ConfigError(msg) => write!(f, "Configuration Error: {}", msg),
            AppError::FileError(msg) => write!(f, "File Error: {}", msg),
            AppError::ParseError(msg) => write!(f, "Parse Error: {}", msg),
            AppError::IoError(msg) => write!(f, "I/O Error: {}", msg),
            AppError::ValidationError(msg) => write!(f, "Validation Error: {}", msg),
        }
    }
}

impl std::error::Error for AppError {}

type AppResult<T> = Result<T, AppError>;

// ============================================================================
// Data Structures
// ============================================================================

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Category {
    id: String,
    name: String,
    grams_per_piece: f64,
    price_per_kg: f64,
}

impl Category {
    fn validate(&self) -> AppResult<()> {
        if self.id.is_empty() {
            return Err(AppError::ValidationError(
                "Category ID cannot be empty".to_string(),
            ));
        }
        if self.name.is_empty() {
            return Err(AppError::ValidationError(
                "Category name cannot be empty".to_string(),
            ));
        }
        if self.grams_per_piece <= 0.0 {
            return Err(AppError::ValidationError(
                format!("Invalid weight for {}: must be positive", self.id),
            ));
        }
        if self.price_per_kg <= 0.0 {
            return Err(AppError::ValidationError(
                format!("Invalid price for {}: must be positive", self.id),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Config {
    categories: Vec<Category>,
}

impl Config {
    fn validate(&self) -> AppResult<()> {
        if self.categories.is_empty() {
            return Err(AppError::ValidationError(
                "Configuration must contain at least one category".to_string(),
            ));
        }

        for category in &self.categories {
            category.validate()?;
        }

        let ids: std::collections::HashSet<_> = self.categories.iter().map(|c| &c.id).collect();
        if ids.len() != self.categories.len() {
            return Err(AppError::ValidationError(
                "Duplicate category IDs found".to_string(),
            ));
        }

        Ok(())
    }
}

// ============================================================================
// CLI Arguments
// ============================================================================

#[derive(Parser)]
#[command(name = "pieceweight")]
#[command(about = "Calculate total weight and price for piece-based sweets orders")]
#[command(author = "Umair Parak")]
#[command(version = "0.1.0")]
struct Cli {
    /// Path to categories configuration file
    #[arg(short, long, default_value = "config/sample_categories.toml")]
    config: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start interactive order entry
    Interactive,

    /// Calculate from CSV file (format: id,count)
    FromCsv { file: PathBuf },
}

// ============================================================================
// Configuration Loading
// ============================================================================

fn load_config(path: &PathBuf) -> AppResult<Config> {
    let content = fs::read_to_string(path).map_err(|e| {
        AppError::FileError(format!("Cannot read config file '{}': {}", path.display(), e))
    })?;

    let config: Config = toml::from_str(&content).map_err(|e| {
        AppError::ConfigError(format!("Invalid TOML format in config file: {}", e))
    })?;

    config.validate()?;
    Ok(config)
}

fn build_category_map(cfg: &Config) -> HashMap<String, Category> {
    cfg.categories
        .iter()
        .cloned()
        .map(|c| (c.id.clone(), c))
        .collect()
}

// ============================================================================
// Calculations
// ============================================================================

fn weight_kg(grams_per_piece: f64, count: u64) -> f64 {
    (grams_per_piece / 1000.0) * count as f64
}

fn price_for(cat: &Category, count: u64) -> f64 {
    let weight = weight_kg(cat.grams_per_piece, count);
    weight * cat.price_per_kg
}

// ============================================================================
// Output Formatting
// ============================================================================

fn print_header(title: &str) {
    println!("\n{}", "=".repeat(80));
    println!("{:^80}", title);
    println!("{}\n", "=".repeat(80));
}

fn print_subheader(title: &str) {
    println!("\n{}", title);
    println!("{}\n", "-".repeat(title.len()));
}

fn print_categories(cfg: &Config) {
    print_subheader("Available Categories");
    println!(
        "{:8} | {:28} | {:10} | {:10}",
        "ID", "Name", "g/piece", "€/kg"
    );
    println!("{}", "-".repeat(70));

    for c in &cfg.categories {
        println!(
            "{:8} | {:28} | {:10.1} | {:10.2}",
            c.id, c.name, c.grams_per_piece, c.price_per_kg
        );
    }
}

// ============================================================================
// Interactive Order
// ============================================================================

fn interactive_order(cfg: &Config) -> AppResult<()> {
    let map = build_category_map(cfg);
    print_header("PIECEWEIGHT - Interactive Order Entry");
    print_categories(cfg);

    println!("\n📝 Enter orders in format: id,count");
    println!("   Example: pistachio,10");
    println!("   Press ENTER on empty line to finish.\n");

    let mut items: Vec<(String, u64)> = Vec::new();

    loop {
        print!("➜ ");
        io::stdout()
            .flush()
            .map_err(|e| AppError::IoError(e.to_string()))?;

        let mut line = String::new();
        io::stdin()
            .read_line(&mut line)
            .map_err(|e| AppError::IoError(format!("Failed to read input: {}", e)))?;
        let line = line.trim();

        if line.is_empty() {
            break;
        }

        // Skip comments
        if line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = line.split(',').map(|s| s.trim()).collect();
        if parts.len() != 2 {
            eprintln!("❌ Invalid format: expected 'id,count', got '{}'", line);
            continue;
        }

        let id = parts[0].to_string();
        let count: u64 = match parts[1].parse() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("❌ Invalid quantity: '{}' is not a valid number", parts[1]);
                continue;
            }
        };

        if count == 0 {
            eprintln!("❌ Quantity must be greater than 0");
            continue;
        }

        if !map.contains_key(&id) {
            eprintln!("❌ Unknown category ID: '{}'. Available IDs:", id);
            for category in &cfg.categories {
                eprintln!("   - {}", category.id);
            }
            continue;
        }

        items.push((id.clone(), count));
        println!("   ✓ Added: {} x{}", map.get(&id).unwrap().name, count);
    }

    if items.is_empty() {
        println!("⚠️  No items entered.");
        return Ok(());
    }

    print_receipt(cfg, &items)?;
    Ok(())
}

// ============================================================================
// CSV Order
// ============================================================================

fn csv_order(cfg: &Config, path: &PathBuf) -> AppResult<()> {
    let mut rdr = csv::Reader::from_path(path)
        .map_err(|e| AppError::FileError(format!("Cannot read CSV file: {}", e)))?;

    let mut items: Vec<(String, u64)> = Vec::new();
    let mut row_num = 0;

    for result in rdr.records() {
        row_num += 1;
        let record = result
            .map_err(|e| AppError::FileError(format!("Error reading CSV row {}: {}", row_num, e)))?;

        let id = record
            .get(0)
            .ok_or_else(|| {
                AppError::ParseError(format!("Row {}: Missing ID column", row_num))
            })?
            .trim()
            .to_string();

        if id.is_empty() {
            return Err(AppError::ValidationError(
                format!("Row {}: ID cannot be empty", row_num),
            ));
        }

        let count: u64 = record
            .get(1)
            .ok_or_else(|| {
                AppError::ParseError(format!("Row {}: Missing count column", row_num))
            })?
            .trim()
            .parse()
            .map_err(|_| {
                AppError::ParseError(format!("Row {}: Invalid count value", row_num))
            })?;

        if count == 0 {
            return Err(AppError::ValidationError(format!(
                "Row {}: Quantity must be greater than 0",
                row_num
            )));
        }

        items.push((id, count));
    }

    if items.is_empty() {
        return Err(AppError::ValidationError(
            "CSV file contains no valid orders".to_string(),
        ));
    }

    print_receipt(cfg, &items)?;
    Ok(())
}

// ============================================================================
// Receipt Generation
// ============================================================================

struct ReceiptRow {
    id: String,
    name: String,
    count: u64,
    weight_kg: f64,
    price: f64,
}

fn print_receipt(cfg: &Config, items: &[(String, u64)]) -> AppResult<()> {
    let map = build_category_map(cfg);
    let mut receipt_rows: Vec<ReceiptRow> = Vec::new();

    let mut total_weight = 0.0;
    let mut total_price = 0.0;
    let mut total_pieces: u64 = 0;

    for (id, count) in items {
        let cat = map.get(id).ok_or_else(|| {
            AppError::ValidationError(format!("Unknown category: {}", id))
        })?;

        let weight = weight_kg(cat.grams_per_piece, *count);
        let price = price_for(cat, *count);

        receipt_rows.push(ReceiptRow {
            id: id.clone(),
            name: cat.name.clone(),
            count: *count,
            weight_kg: weight,
            price,
        });

        total_weight += weight;
        total_price += price;
        total_pieces += *count;
    }

    // Print receipt to console
    print_header("ORDER RECEIPT");

    println!(
        "{:20} {:>8} {:>12} {:>12}",
        "Item", "Qty", "Weight(kg)", "Price(€)"
    );
    println!("{}", "-".repeat(70));

    for row in &receipt_rows {
        println!(
            "{:20} {:>8} {:>12.3} {:>12.2}",
            row.name, row.count, row.weight_kg, row.price
        );
    }

    println!("{}", "-".repeat(70));
    println!(
        "{:20} {:>8} {:>12.3} {:>12.2}",
        "TOTAL", total_pieces, total_weight, total_price
    );

    // Save to CSV
    save_receipt_csv(&receipt_rows, total_pieces, total_weight, total_price)?;

    println!("\n✅ Receipt generated successfully!");
    Ok(())
}

// ============================================================================
// Receipt CSV Generation - Perfectly Aligned
// ============================================================================

fn save_receipt_csv(
    rows: &[ReceiptRow],
    total_pieces: u64,
    total_weight: f64,
    total_price: f64,
) -> AppResult<()> {
    let mut file_content = String::new();

    // Header Section
    file_content.push_str("================================================================================\n");
    file_content.push_str("                      PIECEWEIGHT - ORDER RECEIPT                      \n");
    file_content.push_str("================================================================================\n");
    file_content.push_str(&format!(
        "Date: {:<65}\n",
        chrono::Local::now()
            .format("%Y-%m-%d %H:%M:%S")
            .to_string()
    ));
    file_content.push_str("================================================================================\n\n");

    // Table Header with proper spacing
    file_content.push_str("  ID              Item Name                   Qty      Weight (kg)     Price (€)  \n");
    file_content.push_str("  ───────────────────────────────────────────────────────────────────────────────\n");

    // Data rows with consistent spacing
    for row in rows {
        file_content.push_str(&format!(
            "  {:<12}    {:<24}    {:>6}       {:>12.3}       {:>10.2}\n",
            row.id, row.name, row.count, row.weight_kg, row.price
        ));
    }

    // Separator
    file_content.push_str("  ───────────────────────────────────────────────────────────────────────────────\n");

    // Total row with consistent spacing
    file_content.push_str(&format!(
        "  {:<12}    {:<24}    {:>6}       {:>12.3}       {:>10.2}\n",
        "TOTAL", "", total_pieces, total_weight, total_price
    ));

    file_content.push_str("  ───────────────────────────────────────────────────────────────────────────────\n\n");

    // Summary Statistics Section
    file_content.push_str("  SUMMARY\n");
    file_content.push_str("  ───────────────────────────────────────────────────────────────────────────────\n");
    file_content.push_str(&format!(
        "  Total Items (pieces)           {:>50}\n",
        total_pieces
    ));
    file_content.push_str(&format!(
        "  Total Weight                   {:>45.3} kg\n",
        total_weight
    ));
    file_content.push_str(&format!(
        "  Total Price                    {:>47.2} €\n",
        total_price
    ));

    let avg_price = if total_pieces > 0 {
        total_price / total_pieces as f64
    } else {
        0.0
    };
    file_content.push_str(&format!(
        "  Average Price per Item         {:>47.2} €\n",
        avg_price
    ));

    let avg_weight = if total_pieces > 0 {
        (total_weight * 1000.0) / total_pieces as f64
    } else {
        0.0
    };
    file_content.push_str(&format!(
        "  Average Weight per Item        {:>46.1} g\n",
        avg_weight
    ));

    file_content.push_str("  ───────────────────────────────────────────────────────────────────────────────\n");
    file_content.push_str("================================================================================\n");

    // Write to file
    fs::write("receipt.csv", file_content)
        .map_err(|e| AppError::FileError(format!("Failed to save receipt.csv: {}", e)))?;

    println!("📄 Receipt saved to: receipt.csv");
    Ok(())
}

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() {
    let cli = Cli::parse();

    if let Err(e) = run(cli) {
        eprintln!("\n❌ {}\n", e);
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> AppResult<()> {
    let cfg = load_config(&cli.config)?;

    match cli.command {
        Commands::Interactive => interactive_order(&cfg)?,
        Commands::FromCsv { file } => csv_order(&cfg, &file)?,
    }

    Ok(())
}
