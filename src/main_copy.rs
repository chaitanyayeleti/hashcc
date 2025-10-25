use std::{
    fs::File,
    io::{self, BufReader, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};

use blake3;
use clap::{Parser, Subcommand, ValueEnum};
use colored::*;
use csv;
use indicatif::{ProgressBar, ProgressStyle};
use md5::Md5;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use walkdir::WalkDir;
use hex::encode;
use glob::Pattern;

/// Hashsum - fast, multi-algorithm file hashing utility
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

    /// Exclude files matching glob patterns (comma-separated)
    #[arg(long)]
    exclude: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct HashResult {
    path: String,
    hash: String,
    algo: String,
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

/// Hash bytes using selected algorithm
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
    let mmap_res = unsafe { memmap2::Mmap::map(&file) };
    match mmap_res {
        Ok(map) => Ok(hash_bytes(&map, algo)),
        Err(_) => {
            let mut reader = BufReader::new(file);
            let mut buffer = Vec::new();
            reader.read_to_end(&mut buffer)?;
            Ok(hash_bytes(&buffer, algo))
        }
    }
}

/// Process path recursively (multithreaded) with optional exclusion patterns
fn process_path(
    path: &Path,
    algo: &HashAlgo,
    exclude_patterns: &[Pattern],
    quiet: bool,
) -> io::Result<Vec<HashResult>> {
    let mut results = Vec::new();

    if path.is_file() {
        if !is_excluded(path, exclude_patterns) {
            let hash = hash_file(path, algo)?;
            results.push(HashResult {
                path: path.display().to_string(),
                hash,
                algo: format!("{:?}", algo),
            });
        }
    } else if path.is_dir() {
        let entries: Vec<_> = WalkDir::new(path)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file() && !is_excluded(e.path(), exclude_patterns))
            .collect();

        let pb = ProgressBar::new(entries.len() as u64);
        pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}",
            )
            .unwrap()
            .progress_chars("=>-"),
        );

        let results_mutex = Mutex::new(Vec::new());

        entries.par_iter().for_each(|entry| {
            let file_path = entry.path();
            if let Ok(hash) = hash_file(file_path, algo) {
                let mut vec = results_mutex.lock().unwrap();
                vec.push(HashResult {
                    path: file_path.display().to_string(),
                    hash,
                    algo: format!("{:?}", algo),
                });
            }
            pb.inc(1);
        });

        pb.finish_with_message("Done");
        results = results_mutex.into_inner().unwrap();
    }

    Ok(results)
}

/// Check if a file matches any exclude pattern
fn is_excluded(path: &Path, patterns: &[Pattern]) -> bool {
    let fname = path.to_string_lossy();
    patterns.iter().any(|pat| pat.matches(&fname))
}

/// Output results in chosen format
fn output_results(
    results: &[HashResult],
    format: OutputFormat,
    output_file: Option<&PathBuf>,
    quiet: bool,
) -> io::Result<()> {
    let mut output: Box<dyn Write> = if let Some(file) = output_file {
        Box::new(File::create(file)?)
    } else {
        Box::new(std::io::stdout())
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

/// Verify a CSV hash file
fn verify_hash_file(checksum_file: &Path) -> io::Result<()> {
    let mut rdr = csv::Reader::from_path(checksum_file)?;
    for result in rdr.deserialize::<HashResult>() {
        let record: HashResult = result?;
        let path = Path::new(&record.path);
        if path.exists() {
            let algo = match record.algo.as_str() {
                "Md5" => HashAlgo::Md5,
                "Sha1" => HashAlgo::Sha1,
                "Sha256" => HashAlgo::Sha256,
                "Sha512" => HashAlgo::Sha512,
                "Blake3" => HashAlgo::Blake3,
                _ => HashAlgo::Sha256,
            };
            let hash = hash_file(path, &algo)?;
            if hash == record.hash {
                println!("{} {}", "✅".green(), path.display());
            } else {
                println!("{} {} FAILED", "❌".red(), path.display());
            }
        } else {
            println!("{} {} MISSING", "⚠️".yellow(), path.display());
        }
    }
    Ok(())
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();

    let exclude_patterns: Vec<Pattern> = cli
        .exclude
        .unwrap_or_default()
        .split(',')
        .filter_map(|s| Pattern::new(s).ok())
        .collect();

    match cli.command {
        Commands::Generate { file_path, algo } => {
            if let Some(path) = file_path {
                let results = process_path(&path, &algo, &exclude_patterns, cli.quiet)?;
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
                println!("{} Hash matches!", "✅".green());
            } else {
                println!("{} Hash does not match.", "❌".red());
                println!("Expected: {}", input_hash);
                println!("Actual:   {}", actual_hash);
            }
        }
    }

    Ok(())
}
