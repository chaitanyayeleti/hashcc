use std::{
    fs::File,
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use clap::{Parser, Subcommand, ValueEnum};
use md5::Md5;
use sha1::Sha1;
use sha2::{Sha256, Sha512, Digest};
use blake3;
use memmap2::Mmap;
use walkdir::WalkDir;
use hex::encode;
use rayon::prelude::*;
use serde::{Serialize, Deserialize};
use globset::{Glob, GlobSetBuilder};

#[derive(Parser)]
#[command(name = "hashsum", version, about = "Generate and compare file hashes (SHA, MD5, BLAKE3)")]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Choose output format: text, json, csv
    #[arg(long, default_value = "text")]
    format: OutputFormat,

    /// Suppress normal output
    #[arg(long)]
    quiet: bool,

    /// Save output to a file
    #[arg(long)]
    output: Option<PathBuf>,
}

#[derive(Serialize, Deserialize)]
struct HashResult {
    path: String,
    hash: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate hashes for a file, folder, or stdin
    Generate {
        /// Path to the file or directory (omit for stdin)
        file_path: Option<PathBuf>,

        /// Choose algorithm: md5, sha1, sha256, sha512, blake3
        #[arg(long, value_enum, default_value = "sha256")]
        algo: HashAlgo,

        /// Optional exclude patterns
        #[arg(long)]
        exclude: Vec<String>,
    },

    /// Compare a given hash with a file's hash
    Compare {
        /// The input hash to compare against
        input_hash: String,

        /// Path to the file
        file_path: PathBuf,

        /// Choose algorithm: md5, sha1, sha256, sha512, blake3
        #[arg(long, value_enum, default_value = "sha256")]
        algo: HashAlgo,
    },
}

#[derive(Clone, ValueEnum, Debug)]
enum HashAlgo {
    Md5,
    Sha1,
    Sha256,
    Sha512,
    Blake3,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
    Csv,
}

fn hash_bytes(data: &[u8], algo: &HashAlgo) -> String {
    match algo {
        HashAlgo::Md5 => encode(Md5::digest(data)),
        HashAlgo::Sha1 => encode(Sha1::digest(data)),
        HashAlgo::Sha256 => encode(Sha256::digest(data)),
        HashAlgo::Sha512 => encode(Sha512::digest(data)),
        HashAlgo::Blake3 => blake3::hash(data).to_hex().to_string(),
    }
}

/// Efficiently hash a file using mmap (for large files) or buffered read
fn hash_file(path: &Path, algo: &HashAlgo) -> io::Result<String> {
    let file = File::open(path)?;

    if let Ok(map) = unsafe { Mmap::map(&file) } {
        Ok(hash_bytes(&map, algo))
    } else {
        let mut reader = BufReader::new(file);
        let mut buffer = Vec::new();
        reader.read_to_end(&mut buffer)?;
        Ok(hash_bytes(&buffer, algo))
    }
}

fn process_path(path: &Path, algo: &HashAlgo, exclude_files: &[String]) -> io::Result<Vec<HashResult>> {
    let mut results = Vec::new();

    // Compile exclude patterns using globset
    let mut builder = GlobSetBuilder::new();
    for pat in exclude_files {
        builder.add(Glob::new(pat).unwrap());
    }
    let glob_set = builder.build().unwrap();

    if path.is_file() {
        if !glob_set.is_match(path) {
            let hash = hash_file(path, algo)?;
            results.push(HashResult {
                path: path.display().to_string(),
                hash,
            });
        }
    } else if path.is_dir() {
        let entries: Vec<_> = WalkDir::new(path)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file() && !glob_set.is_match(e.path()))
            .collect();

        let results_mutex = Mutex::new(Vec::new());

        entries.par_iter().for_each(|entry| {
            let file_path = entry.path();
            if let Ok(hash) = hash_file(file_path, algo) {
                let mut vec = results_mutex.lock().unwrap();
                vec.push(HashResult {
                    path: file_path.display().to_string(),
                    hash,
                });
            }
        });

        results = results_mutex.into_inner().unwrap();
    }

    Ok(results)
}

fn output_results(results: &[HashResult], format: OutputFormat, output_file: Option<&PathBuf>, quiet: bool) -> io::Result<()> {
    let mut output: Box<dyn Write> = if let Some(file) = output_file {
        Box::new(File::create(file)?)
    } else {
        Box::new(io::stdout())
    };

    match format {
        OutputFormat::Text => {
            if !quiet {
                for r in results {
                    writeln!(output, "{}  {}", r.hash, r.path)?;
                }
            }
        }
        OutputFormat::Json => {
            serde_json::to_writer_pretty(&mut output, results)?;
        }
        OutputFormat::Csv => {
            let mut wtr = csv::Writer::from_writer(output);
            for r in results {
                wtr.serialize(r)?;
            }
            wtr.flush()?;
        }
    }

    Ok(())
}

fn verify_hash_file(checksum_file: &Path) -> io::Result<()> {
    let mut rdr = csv::Reader::from_path(checksum_file)?;
    for result in rdr.deserialize::<HashResult>() {
        let record = result?;
        let path = Path::new(&record.path);
        if path.exists() {
            let hash = hash_file(path, &HashAlgo::Sha256)?; // could store algo in CSV later
            if hash == record.hash {
                println!("✅ {} OK", path.display());
            } else {
                println!("❌ {} FAILED", path.display());
            }
        } else {
            println!("⚠️ {} MISSING", path.display());
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Generate { file_path, algo, exclude } => {
            if let Some(path) = file_path {
                let results = process_path(&path, &algo, &exclude)?;
                output_results(&results, cli.format, cli.output.as_ref(), cli.quiet)?;
            } else {
                // Read from stdin
                let mut data = Vec::new();
                io::stdin().read_to_end(&mut data)?;
                println!("{}", hash_bytes(&data, &algo));
            }
        }
        Commands::Compare { input_hash, file_path, algo } => {
            let actual_hash = hash_file(&file_path, &algo)?;
            if actual_hash.eq_ignore_ascii_case(&input_hash) {
                println!("✅ Hash matches!");
            } else {
                println!("❌ Hash does not match.");
                println!("Expected: {}", input_hash);
                println!("Actual:   {}", actual_hash);
            }
        }
    }

    Ok(())
}
