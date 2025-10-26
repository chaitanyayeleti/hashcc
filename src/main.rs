use std::{
    fs::File,
    io::{self, BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
};
use std::process;

use clap::{Parser, Subcommand, ValueEnum, ValueHint};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};
use blake3;
use memmap2::Mmap;
use walkdir::WalkDir;
use hex::encode;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use globset::{Glob, GlobSetBuilder};

#[derive(Parser)]
#[command(
    name = "hashcc",
    version,
    about = "Generate, compare, and verify file hashes",
    long_about = "A fast, parallel hashing utility supporting MD5, SHA-1, SHA-256, SHA-512, and BLAKE3.\n\n• Generate hashes for files/dirs or stdin\n• Compare a file to a given hash\n• Verify a CSV of computed hashes",
    after_help = "EXAMPLES:\n  # Hash a single file (SHA-256 default)\n  hashcc generate /path/to/file\n\n  # Hash a directory (CSV output) and verify later\n  hashcc generate --format csv /path/to/dir > checksums.csv\n  hashcc verify checksums.csv --algo sha256\n\n  # Read from stdin and hash with blake3\n  echo -n 'hello' | hashcc generate --algo blake3\n\n  # Compare a file to a known hash\n  hashcc compare --algo sha512 <HASH> /path/to/file\n\n  # Exclude patterns when hashing a directory\n  hashcc generate --exclude '**/*.tmp' --exclude 'node_modules/**' /path/to/dir"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Choose output format: text, json, csv
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,

    /// Suppress normal output
    #[arg(long)]
    quiet: bool,

    /// Save output to a file
    #[arg(long, value_name = "FILE", value_hint = ValueHint::FilePath)]
    output: Option<PathBuf>,

    /// Allow using weak algorithms (md5, sha1)
    #[arg(long)]
    allow_weak: bool,
}

fn verify_sumfile(checksum_file: &Path, algo: &HashAlgo, base_dir: Option<&Path>, allow_absolute: bool, quiet: bool) -> io::Result<i32> {
    let file = File::open(checksum_file)?;
    let reader = BufReader::new(file);
    let mut ok = 0usize;
    let mut failed = 0usize;
    let mut missing = 0usize;
    let mut invalid_path = 0usize;
    let mut errors = 0usize;
    for line in reader.lines() {
        match line {
            Ok(l) => {
                let trimmed = l.trim_end();
                if trimmed.is_empty() { continue; }
                let mut parts = trimmed.splitn(2, |c: char| c.is_whitespace());
                let hash = parts.next().unwrap_or("");
                let rest = parts.next().unwrap_or("").trim_start();
                if hash.is_empty() || rest.is_empty() { errors += 1; if !quiet { eprintln!("❌ invalid sumfile line: {}", trimmed); } continue; }
                let record = HashResult { path: rest.to_string(), hash: hash.to_string() };
                let raw_path = Path::new(&record.path);
                if !allow_absolute && raw_path.is_absolute() { invalid_path += 1; if !quiet { eprintln!("❌ absolute path not allowed: {}", raw_path.display()); } continue; }
                let resolved = if let Some(base) = base_dir { if raw_path.is_absolute() { raw_path.to_path_buf() } else { base.join(raw_path) } } else { raw_path.to_path_buf() };
                if let Some(base) = base_dir {
                    let Ok(base_can) = base.canonicalize() else { errors += 1; if !quiet { eprintln!("❌ cannot canonicalize base dir: {}", base.display()); } continue; };
                    if let Ok(res_can) = resolved.canonicalize() { if !res_can.starts_with(&base_can) { invalid_path += 1; if !quiet { eprintln!("❌ path escapes base dir: {}", resolved.display()); } continue; } }
                }
                if resolved.exists() {
                    match hash_file(&resolved, algo) {
                        Ok(h) => {
                            let ok_cmp = constant_time_eq(h.as_bytes(), record.hash.as_bytes());
                            if ok_cmp { ok += 1; if !quiet { println!("✅ {} OK", resolved.display()); } }
                            else { failed += 1; println!("❌ {} FAILED", resolved.display()); }
                        }
                        Err(e) => { errors += 1; eprintln!("❌ {} ERROR: {}", resolved.display(), e); }
                    }
                } else { missing += 1; println!("⚠️ {} MISSING", resolved.display()); }
            }
            Err(e) => { errors += 1; eprintln!("❌ read error: {}", e); }
        }
    }
    if !quiet { eprintln!("Summary: OK={} FAILED={} MISSING={} INVALID_PATH={} ERROR={}", ok, failed, missing, invalid_path, errors); }
    Ok(if failed == 0 && missing == 0 && invalid_path == 0 && errors == 0 { 0 } else { 1 })
}

#[derive(Serialize, Deserialize)]
struct HashResult {
    path: String,
    hash: String,
}

#[derive(Subcommand)]
enum Commands {
    #[command(visible_aliases = ["gen"])]
    Generate {
        #[arg(value_name = "PATH", value_hint = ValueHint::AnyPath)]
        file_path: Option<PathBuf>,
        #[arg(long, value_enum, default_value = "sha256")]
        algo: HashAlgo,
        #[arg(long, value_name = "GLOB", num_args = 1..)]
        exclude: Vec<String>,
        #[arg(long)]
        progress: bool,
        #[arg(long)]
        archives: bool,
    },
    #[command(visible_aliases = ["cmp"])]
    Compare {
        #[arg(value_name = "HASH")]
        input_hash: String,
        #[arg(value_name = "FILE", value_hint = ValueHint::FilePath)]
        file_path: PathBuf,
        #[arg(long, value_enum, default_value = "sha256")]
        algo: HashAlgo,
    },
    #[command(visible_aliases = ["ver", "check"])]
    Verify {
        #[arg(value_name = "CSV", value_hint = ValueHint::FilePath)]
        checksum_file: PathBuf,
        #[arg(long, value_enum, default_value = "sha256")]
        algo: HashAlgo,
        #[arg(long, value_name = "DIR", value_hint = ValueHint::DirPath)]
        base_dir: Option<PathBuf>,
        #[arg(long)]
        allow_absolute: bool,
        #[arg(long)]
        sumfile: bool,
    },
}

#[derive(Clone, ValueEnum, Debug)]
enum HashAlgo { Md5, Sha1, Sha256, Sha512, Blake3 }

#[derive(Clone, ValueEnum)]
enum OutputFormat { Text, Json, Csv, Sumfile }

fn hash_bytes(data: &[u8], algo: &HashAlgo) -> String {
    match algo {
        HashAlgo::Md5 => encode(Md5::digest(data)),
        HashAlgo::Sha1 => encode(Sha1::digest(data)),
        HashAlgo::Sha256 => encode(Sha256::digest(data)),
        HashAlgo::Sha512 => encode(Sha512::digest(data)),
        HashAlgo::Blake3 => blake3::hash(data).to_hex().to_string(),
    }
}

fn hash_file(path: &Path, algo: &HashAlgo) -> io::Result<String> {
    let file = File::open(path)?;
    if let Ok(map) = unsafe { Mmap::map(&file) } {
        Ok(hash_bytes(&map, algo))
    } else {
        hash_reader(BufReader::new(file), algo)
    }
}

fn hash_reader<R: Read>(mut reader: R, algo: &HashAlgo) -> io::Result<String> {
    let mut buf = [0u8; 64 * 1024];
    match algo {
        HashAlgo::Md5 => { let mut h = Md5::new(); loop { let n = reader.read(&mut buf)?; if n == 0 { break; } h.update(&buf[..n]); } Ok(encode(h.finalize())) }
        HashAlgo::Sha1 => { let mut h = Sha1::new(); loop { let n = reader.read(&mut buf)?; if n == 0 { break; } h.update(&buf[..n]); } Ok(encode(h.finalize())) }
        HashAlgo::Sha256 => { let mut h = Sha256::new(); loop { let n = reader.read(&mut buf)?; if n == 0 { break; } h.update(&buf[..n]); } Ok(encode(h.finalize())) }
        HashAlgo::Sha512 => { let mut h = Sha512::new(); loop { let n = reader.read(&mut buf)?; if n == 0 { break; } h.update(&buf[..n]); } Ok(encode(h.finalize())) }
        HashAlgo::Blake3 => { let mut h = blake3::Hasher::new(); loop { let n = reader.read(&mut buf)?; if n == 0 { break; } h.update(&buf[..n]); } Ok(h.finalize().to_hex().to_string()) }
    }
}

fn is_weak_algo(algo: &HashAlgo) -> bool { matches!(algo, HashAlgo::Md5 | HashAlgo::Sha1) }
fn expected_hex_len(algo: &HashAlgo) -> usize { match algo { HashAlgo::Md5 => 32, HashAlgo::Sha1 => 40, HashAlgo::Sha256 => 64, HashAlgo::Sha512 => 128, HashAlgo::Blake3 => 64 } }
fn is_valid_hex(s: &str) -> bool { !s.is_empty() && s.chars().all(|c| c.is_ascii_hexdigit()) }
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool { let len = a.len().max(b.len()); let mut diff: u8 = 0; for i in 0..len { diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0); } diff == 0 && a.len() == b.len() }

fn process_path(path: &Path, algo: &HashAlgo, exclude_files: &[String], archives: bool, progress: bool) -> io::Result<Vec<HashResult>> {
    #[cfg(not(feature = "archives"))] let _ = archives;
    #[cfg(not(feature = "progress"))] let _ = progress;
    let mut builder = GlobSetBuilder::new();
    for pat in exclude_files { builder.add(Glob::new(pat).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, format!("invalid glob pattern '{}': {}", pat, e)))?); }
    let glob_set = builder.build().map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let mut results = Vec::new();
    if path.is_file() {
        if !glob_set.is_match(path) { results.push(HashResult { path: path.display().to_string(), hash: hash_file(path, algo)? }); }
    } else if path.is_dir() {
        let entries: Vec<_> = WalkDir::new(path).into_iter().filter_map(Result::ok).filter(|e| e.file_type().is_file() && !glob_set.is_match(e.path())).collect();
        #[cfg(feature = "progress")] let pb = if progress { Some(indicatif::ProgressBar::new_spinner()) } else { None };
        #[cfg(feature = "progress")] if let Some(ref pb) = pb { pb.enable_steady_tick(std::time::Duration::from_millis(100)); pb.set_message("Hashing..."); }
        results = entries.par_iter().flat_map_iter(|entry| {
            let file_path = entry.path();
            #[cfg(feature = "progress")] if let Some(ref pb) = pb { pb.set_message(file_path.display().to_string()); }
            #[cfg(feature = "archives")] if archives { if let Some(ext) = file_path.extension().and_then(|e| e.to_str()).map(|s| s.to_ascii_lowercase()) { if ext == "zip" { return match hash_zip(file_path, algo) { Ok(v) => v, Err(_) => Vec::new() }; } if ext == "tar" { return match hash_tar_like(file_path, algo, false) { Ok(v) => v, Err(_) => Vec::new() }; } if ext == "gz" { let name = file_path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase(); if name.ends_with(".tar.gz") || name.ends_with(".tgz") { return match hash_tar_like(file_path, algo, true) { Ok(v) => v, Err(_) => Vec::new() }; } } } }
            match hash_file(file_path, algo) { Ok(hash) => vec![HashResult { path: file_path.display().to_string(), hash }], Err(_) => Vec::new() }
        }).collect();
        #[cfg(feature = "progress")] if let Some(pb) = pb { pb.finish_and_clear(); }
    }
    results.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(results)
}

#[cfg(feature = "archives")]
fn hash_zip(path: &Path, algo: &HashAlgo) -> io::Result<Vec<HashResult>> {
    let file = File::open(path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if entry.is_dir() { continue; }
        let hash = hash_reader(&mut io::Read::take(&mut entry, u64::MAX), algo)?;
        out.push(HashResult { path: format!("{}!/{}", path.display(), entry.name()), hash });
    }
    Ok(out)
}

#[cfg(feature = "archives")]
fn hash_tar_like(path: &Path, algo: &HashAlgo, gz: bool) -> io::Result<Vec<HashResult>> {
    let file = File::open(path)?;
    let reader: Box<dyn Read> = if gz { Box::new(flate2::read::GzDecoder::new(file)) } else { Box::new(file) };
    let mut archive = tar::Archive::new(reader);
    let mut out = Vec::new();
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.header().entry_type().is_dir() { continue; }
        let hash = hash_reader(&mut entry, algo)?;
        let inner = entry.path().ok().and_then(|p| p.into_owned().into_os_string().into_string().ok()).unwrap_or_else(|| "<unknown>".to_string());
        out.push(HashResult { path: format!("{}!/{}", path.display(), inner), hash });
    }
    Ok(out)
}

fn output_results(results: &[HashResult], format: OutputFormat, output_file: Option<&PathBuf>, quiet: bool) -> io::Result<()> {
    let mut output: Box<dyn Write> = if let Some(file) = output_file { Box::new(File::create(file)?) } else { Box::new(io::stdout()) };
    match format {
        OutputFormat::Text | OutputFormat::Sumfile => { if !quiet { for r in results { writeln!(output, "{}  {}", r.hash, r.path)?; } } }
        OutputFormat::Json => { serde_json::to_writer_pretty(&mut output, results)?; }
        OutputFormat::Csv => { let mut wtr = csv::Writer::from_writer(output); for r in results { wtr.serialize(r)?; } wtr.flush()?; }
    }
    Ok(())
}

fn verify_hash_file(checksum_file: &Path, algo: &HashAlgo, base_dir: Option<&Path>, allow_absolute: bool, quiet: bool) -> io::Result<i32> {
    let mut rdr = csv::Reader::from_path(checksum_file)?;
    let mut ok = 0usize; let mut failed = 0usize; let mut missing = 0usize; let mut invalid_path = 0usize; let mut errors = 0usize;
    for result in rdr.deserialize::<HashResult>() {
        match result {
            Ok(record) => {
                let raw_path = Path::new(&record.path);
                if !allow_absolute && raw_path.is_absolute() { invalid_path += 1; if !quiet { eprintln!("❌ absolute path not allowed: {}", raw_path.display()); } continue; }
                let resolved = if let Some(base) = base_dir { if raw_path.is_absolute() { raw_path.to_path_buf() } else { base.join(raw_path) } } else { raw_path.to_path_buf() };
                if let Some(base) = base_dir {
                    let Ok(base_can) = base.canonicalize() else { errors += 1; if !quiet { eprintln!("❌ cannot canonicalize base dir: {}", base.display()); } continue; };
                    if let Ok(res_can) = resolved.canonicalize() { if !res_can.starts_with(&base_can) { invalid_path += 1; if !quiet { eprintln!("❌ path escapes base dir: {}", resolved.display()); } continue; } }
                }
                if resolved.exists() {
                    match hash_file(&resolved, algo) {
                        Ok(hash) => { if constant_time_eq(hash.as_bytes(), record.hash.as_bytes()) { ok += 1; if !quiet { println!("✅ {} OK", resolved.display()); } } else { failed += 1; println!("❌ {} FAILED", resolved.display()); } }
                        Err(e) => { errors += 1; eprintln!("❌ {} ERROR: {}", resolved.display(), e); }
                    }
                } else { missing += 1; println!("⚠️ {} MISSING", resolved.display()); }
            }
            Err(e) => { errors += 1; eprintln!("❌ invalid CSV row: {}", e); }
        }
    }
    if !quiet { eprintln!("Summary: OK={} FAILED={} MISSING={} INVALID_PATH={} ERROR={}", ok, failed, missing, invalid_path, errors); }
    Ok(if failed == 0 && missing == 0 && invalid_path == 0 && errors == 0 { 0 } else { 1 })
}

fn main() -> io::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Generate { file_path, algo, exclude, progress, archives } => {
            if is_weak_algo(&algo) && !cli.allow_weak { eprintln!("Refusing to use weak algorithm {:?}. Pass --allow-weak to proceed.", algo); process::exit(2); }
            if progress { eprintln!("Note: --progress requires building with the 'progress' feature. Proceeding without progress."); }
            if archives { eprintln!("Note: --archives requires building with the 'archives' feature. Archive hashing is disabled in this build."); }
            if let Some(path) = file_path {
                let results = process_path(&path, &algo, &exclude, archives, progress)?;
                output_results(&results, cli.format, cli.output.as_ref(), cli.quiet)?;
            } else {
                let hash = hash_reader(io::stdin().lock(), &algo)?;
                println!("{}", hash);
            }
        }
        Commands::Compare { input_hash, file_path, algo } => {
            if is_weak_algo(&algo) && !cli.allow_weak { eprintln!("Refusing to use weak algorithm {:?}. Pass --allow-weak to proceed.", algo); process::exit(2); }
            let expected_len = expected_hex_len(&algo);
            if input_hash.len() != expected_len || !is_valid_hex(&input_hash) { eprintln!("Invalid {}-bit hash: expected {} hex chars", expected_len * 4, expected_len); process::exit(2); }
            let actual_hash = hash_file(&file_path, &algo)?;
            if constant_time_eq(actual_hash.as_bytes(), input_hash.as_bytes()) { println!("✅ Hash matches!"); } else { println!("❌ Hash does not match."); println!("Expected: {}", input_hash); println!("Actual:   {}", actual_hash); process::exit(1); }
        }
        Commands::Verify { checksum_file, algo, base_dir, allow_absolute, sumfile } => {
            if is_weak_algo(&algo) && !cli.allow_weak { eprintln!("Refusing to use weak algorithm {:?}. Pass --allow-weak to proceed.", algo); process::exit(2); }
            let code = if sumfile { verify_sumfile(&checksum_file, &algo, base_dir.as_deref(), allow_absolute, cli.quiet)? } else { verify_hash_file(&checksum_file, &algo, base_dir.as_deref(), allow_absolute, cli.quiet)? };
            if code != 0 { process::exit(code); }
        }
    }
    Ok(())
}
