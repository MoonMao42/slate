use clap::{Parser, Subcommand};
use themectl::ThemeResult;

/// A zero-configuration terminal theme switcher
/// Apply your favorite theme to Ghostty, Starship, bat and other terminal tools
/// with a single command. No config needed.
#[derive(Parser)]
#[command(name = "themectl")]
#[command(version = "0.1.0")]
#[command(about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Apply a theme to all detected tools
    #[command(about = "Apply a theme")]
    Set {
        /// Name of the theme to apply (e.g., catppuccin-mocha, tokyo-night-dark)
        #[arg(value_name = "THEME")]
        theme: String,

        /// Show detection and modification details
        #[arg(short, long)]
        verbose: bool,
    },
}

fn main() -> ThemeResult<()> {
    // Initialize color-eyre for better error formatting
    let _ = color_eyre::install();

    let args = Args::parse();

    match args.command {
        Commands::Set { theme, verbose } => {
            // Placeholder: will be implemented in 
            if verbose {
                eprintln!("Verbose mode: ON");
            }
            println!("Would apply theme: {}", theme);
            Ok(())
        }
    }
}
