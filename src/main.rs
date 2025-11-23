use archdrop::server;
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

// Clap reads this struct and creates CLI 
#[derive(Parser)] // generates arg parsing code at compile time
#[command(name = "archdrop")] // name in --help
#[command(about = "Secure file transfer")] // desc in --help
struct Cli {
    // subcommands
    #[command(subcommand)] 
    command: Commands,
}


// set a enum for possible future commands
#[derive(Subcommand)]
enum Commands {

    Send {
        #[arg(help = "Path to file to send")]
        path: PathBuf, // PathBuf for typesafe paths 
    },
}


#[tokio::main]
async fn main() {
    
    // test_encryption();
    // Reads std::env::args(), matches against struct def
    let cli = Cli::parse();

    match cli.command {
        Commands::Send { path } => {

            // PathBuf.exits(); Check for file before spinning up
            // fail fast on no file
            if !path.exists() {
                // file.display() formats paths
                eprintln!("Error: File not found: {}", path.display());
                std::process::exit(1);
            }

            // sending folder
            let (file_to_send, cleanup_path) = if path.is_dir() {
                let zip_path = create_zip_from_dir(&path).await.unwrap();
                (zip_path.clone(), Some(zip_path)) 
            } else {
                // singe file 
                (path, None)
            };
            
            server::start_server(file_to_send).await.unwrap();

            // cleanup temp zip
            if let Some(temp_path) = cleanup_path {
                let _ = tokio::fs::remove_file(temp_path).await;
            }
        }
    }
}

async fn create_zip_from_dir(dir: &Path) -> Result<PathBuf, Box<dyn std::error::Error>> {
    todo!()
}
