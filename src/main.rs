use rusqlite::{params, Connection, Result as SqlResult};
use std::env;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufReader};
use csv::{Reader, ReaderBuilder, Writer, WriterBuilder};
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::time::{Duration, Instant};
use std::collections::HashSet;

// Increase batch size for better performance
const BATCH_SIZE: usize = 10000;
// How often to update the progress bar (in records)
const PROGRESS_UPDATE_INTERVAL: u64 = 10000;
// How often to commit transactions (in batches)
const COMMIT_INTERVAL: usize = 5;

fn determine_table_and_query(conn: &Connection) -> SqlResult<(String, String)> {
    // Check if METADATA table exists (preferred)
    let metadata_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='METADATA')",
        [],
        |row| row.get(0)
    )?;

    if metadata_exists {
        return Ok(("METADATA".to_string(), 
            "SELECT EXISTS(SELECT 1 FROM METADATA WHERE sha1 = ? OR md5 = ?)".to_string()));
    }

    // Check if FILE view/table exists
    let file_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE (type='table' OR type='view') AND name='FILE')",
        [],
        |row| row.get(0)
    )?;

    if file_exists {
        return Ok(("FILE".to_string(), 
            "SELECT EXISTS(SELECT 1 FROM FILE WHERE sha1 = ? OR md5 = ?)".to_string()));
    }

    Err(rusqlite::Error::QueryReturnedNoRows)
}

// Add a function to check if indexes exist and create them if needed
fn ensure_indexes(conn: &Connection, table_name: &str) -> SqlResult<()> {
    // Check if indexes exist for the table
    let sha1_index_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?)",
        params![format!("{}_sha1_idx", table_name)],
        |row| row.get(0)
    )?;
    
    let md5_index_exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='index' AND name=?)",
        params![format!("{}_md5_idx", table_name)],
        |row| row.get(0)
    )?;
    
    // Create indexes if they don't exist
    if !sha1_index_exists {
        println!("Creating index on sha1 column...");
        conn.execute(
            &format!("CREATE INDEX IF NOT EXISTS {}_sha1_idx ON {} (sha1)", table_name, table_name),
            []
        )?;
    }
    
    if !md5_index_exists {
        println!("Creating index on md5 column...");
        conn.execute(
            &format!("CREATE INDEX IF NOT EXISTS {}_md5_idx ON {} (md5)", table_name, table_name),
            []
        )?;
    }
    
    Ok(())
}

// Helper function to process a batch of records
fn process_batch(
    batch: &mut Vec<csv::StringRecord>,
    stmt: &mut rusqlite::Statement,
    known_writer: &mut Writer<File>,
    unknown_writer: &mut Writer<File>,
    known_count: &mut u64,
    unknown_count: &mut u64,
    empty_hash_count: &mut u64,
    error_count: &mut u64,
    processed_hashes: &mut HashSet<String>,

) -> Result<(), Box<dyn Error>> {
    for record in batch.iter() {
        // Extension filtering is now done before adding to batch, so we don't need to check here
        
        let md5 = record.get(6).unwrap_or("").trim();
        let sha1 = record.get(7).unwrap_or("").trim();
        
        if md5.is_empty() && sha1.is_empty() {
            unknown_writer.write_record(record.iter())?;
            *unknown_count += 1;
            *empty_hash_count += 1;
            continue;
        }

        // Create a hash key using SHA-1 (preferred) or MD5
        let hash_key = if !sha1.is_empty() { sha1.to_string() } else { md5.to_string() };
        
        // Skip if we've already processed this hash
        if !processed_hashes.insert(hash_key) {
            continue;
        }

        let is_known: bool = match stmt.query_row(
            params![
                if !sha1.is_empty() { sha1 } else { md5 },
                if !md5.is_empty() { md5 } else { sha1 }
            ],
            |row| row.get::<_, bool>(0)
        ) {
            Ok(result) => result,
            Err(e) => {
                *error_count += 1;
                if *error_count <= 5 {
                    // Only print the first few errors to avoid flooding the console
                    eprintln!("Query error: {} (sha1={}, md5={})", e, sha1, md5);
                }
                false
            }
        };

        if is_known {
            known_writer.write_record(record.iter())?;
            *known_count += 1;
        } else {
            unknown_writer.write_record(record.iter())?;
            *unknown_count += 1;
        }
    }
    
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let start_time = Instant::now();
    
    let args: Vec<String> = env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: {} <database.sqlite> <filelist.csv> [extensions...]", args[0]);
        eprintln!("Example: {} db.sqlite files.csv exe dll sys", args[0]);
        std::process::exit(1);
    }
    let db_path = &args[1];
    let csv_path = &args[2];
    
    // Process optional extensions
    let extensions = if args.len() > 3 {
        let exts: Vec<String> = args[3..].iter()
            .map(|ext| ext.trim_start_matches('.')
                .to_lowercase()
                .to_string())
            .collect();
        println!("Filtering for extensions: {}", exts.join(", "));
        Some(exts)
    } else {
        None
    };

    println!("Opening database: {}", db_path);
    let mut conn = Connection::open(db_path)?;
    
    // Enable performance optimizations
    println!("Applying SQLite performance optimizations...");
    conn.execute_batch("
        PRAGMA synchronous = OFF;
        PRAGMA journal_mode = MEMORY;
        PRAGMA cache_size = -2000000;
        PRAGMA temp_store = MEMORY;
        PRAGMA mmap_size = 30000000000;
    ")?;

    // Determine which table to use and get appropriate query
    let (table_name, query) = determine_table_and_query(&conn)
        .map_err(|_| "Error: Database must contain either a METADATA table or FILE view with sha1 column")?;
    println!("Using table/view: {}", table_name);
    
    // Ensure indexes exist for better query performance
    match ensure_indexes(&conn, &table_name) {
        Ok(_) => println!("Indexes verified."),
        Err(e) => println!("Warning: Could not create indexes: {}", e),
    }

    println!("Opening CSV file: {}", csv_path);
    let mut rdr = Reader::from_path(csv_path)?;
    let headers = rdr.headers()?.clone();

    // First pass: Count total records and unique hashes
    println!("Scanning CSV for total records and unique hashes...");
    let file = File::open(csv_path)?;
    let reader = BufReader::new(file);
    let total_records = io::read_to_string(reader)?.
        lines()
        .count() as u64 - 1; // Subtract 1 for header

    // Create a HashSet to track unique hashes
    let mut processed_hashes = HashSet::new();
    
    // Pre-scan to count unique hashes
    let mut pre_scan_rdr = ReaderBuilder::new()
        .buffer_capacity(1024 * 1024) // 1MB buffer for reading
        .from_path(csv_path)?;
    
    let pre_scan_pb = ProgressBar::new(total_records);
    pre_scan_pb.set_style(ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")?  
        .progress_chars("##-"));
    pre_scan_pb.set_message("Scanning for unique hashes...");
    
    // Skip the header
    pre_scan_rdr.headers()?;
    
    // Count unique hashes
    let mut scanned_count = 0;
    for result in pre_scan_rdr.records() {
        match result {
            Ok(record) => {
                let md5 = record.get(6).unwrap_or("").trim();
                let sha1 = record.get(7).unwrap_or("").trim();
                
                if !md5.is_empty() || !sha1.is_empty() {
                    // Create a hash key using SHA-1 (preferred) or MD5
                    let hash_key = if !sha1.is_empty() { sha1.to_string() } else { md5.to_string() };
                    processed_hashes.insert(hash_key);
                }
                
                scanned_count += 1;
                if scanned_count % 10000 == 0 {
                    pre_scan_pb.set_position(scanned_count);
                }
            },
            Err(e) => {
                eprintln!("Error reading CSV record during pre-scan: {}", e);
            }
        }
    }
    
    pre_scan_pb.finish_with_message(format!("Found {} unique hashes in {} total records", 
        processed_hashes.len(), total_records));
    
    // Count how many unique hashes match the extension filter
    let mut extension_filtered_hashes = HashSet::new();
    let extension_filter_pb = ProgressBar::new(processed_hashes.len() as u64);
    
    if let Some(exts) = &extensions {
        println!("Filtering unique hashes by extension...");
        extension_filter_pb.set_style(ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")?  
            .progress_chars("##-"));
        extension_filter_pb.set_message("Filtering by extension...");
        
        // Reopen the CSV file to scan for extension matches
        let mut ext_scan_rdr = ReaderBuilder::new()
            .buffer_capacity(1024 * 1024) // 1MB buffer for reading
            .from_path(csv_path)?;
        
        // Skip the header
        ext_scan_rdr.headers()?;
        
        let mut ext_scanned_count = 0;
        for result in ext_scan_rdr.records() {
            match result {
                Ok(record) => {
                    let md5 = record.get(6).unwrap_or("").trim();
                    let sha1 = record.get(7).unwrap_or("").trim();
                    
                    // Skip records with empty hashes
                    if md5.is_empty() && sha1.is_empty() {
                        continue;
                    }
                    
                    // Create a hash key using SHA-1 (preferred) or MD5
                    let hash_key = if !sha1.is_empty() { sha1.to_string() } else { md5.to_string() };
                    
                    // Only process if this is a unique hash we haven't filtered yet
                    if processed_hashes.contains(&hash_key) && !extension_filtered_hashes.contains(&hash_key) {
                        // Get the extension directly from the Extension column
                        let file_ext = if let Some(ext_index) = headers.iter().position(|h| h.eq_ignore_ascii_case("Extension")) {
                            record.get(ext_index).unwrap_or("").trim().to_lowercase()
                        } else {
                            // Fallback to index 2 if Extension column not found
                            record.get(2).unwrap_or("").trim().to_lowercase()
                        };
                        
                        // Check if extension matches
                        let normalized_ext = file_ext.trim_start_matches('.');
                        if exts.iter().any(|ext| {
                            let ext_normalized = ext.trim_start_matches('.');
                            ext_normalized.eq_ignore_ascii_case(normalized_ext)
                        }) {
                            extension_filtered_hashes.insert(hash_key);
                        }
                    }
                    
                    ext_scanned_count += 1;
                    if ext_scanned_count % 10000 == 0 {
                        extension_filter_pb.set_position(extension_filtered_hashes.len() as u64);
                    }
                },
                Err(e) => {
                    eprintln!("Error reading CSV record during extension filtering: {}", e);
                }
            }
        }
        
        extension_filter_pb.finish_with_message(format!("Found {} hashes matching extension filter", 
            extension_filtered_hashes.len()));
        
        // Replace the processed_hashes with only those that match the extension filter
        processed_hashes = extension_filtered_hashes;
    }
    
    // Clear the HashSet to reuse it during actual processing
    let unique_hash_count = processed_hashes.len() as u64;
    processed_hashes.clear();

    // Create multi-progress display for better visualization
    let mp = MultiProgress::new();
    
    // Main progress bar for overall progress (now based on filtered unique hashes)
    let pb = mp.add(ProgressBar::new(unique_hash_count));
    pb.set_style(ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg} - ETA: {eta_precise}")?  
        .progress_chars("##-"));
    pb.set_message("Processing filtered hashes...");
    
    // Status bar for statistics
    let status_bar = mp.add(ProgressBar::new(100));
    status_bar.set_style(ProgressStyle::default_bar()
        .template("Known: {prefix} | Unknown: {msg} | Unique: {pos}/{len} | Total: {per_sec}")?); 

    // Configure CSV writers with performance options
    let mut known_writer = WriterBuilder::new()
        .buffer_capacity(65536) // 64KB buffer
        .from_path("known_software.csv")?;
    let mut unknown_writer = WriterBuilder::new()
        .buffer_capacity(65536) // 64KB buffer
        .from_path("unknown_software.csv")?;
    known_writer.write_record(&headers)?;
    unknown_writer.write_record(&headers)?;

    let mut known_count = 0;
    let mut unknown_count = 0;
    let mut empty_hash_count = 0;
    let mut error_count = 0;
    let mut last_update_time = Instant::now();
    let mut last_update_count = 0;
    let mut records_processed = 0;
    let mut unique_processed = 0;
    
    // Reopen the CSV file for streaming processing
    let mut rdr = ReaderBuilder::new()
        .buffer_capacity(1024 * 1024) // 1MB buffer for reading
        .from_path(csv_path)?;
    
    // Skip the header
    rdr.headers()?;
    
    println!("Starting batch processing...");
    
    // Process in batches with periodic transaction commits
    let mut batch_records = Vec::with_capacity(BATCH_SIZE);
    let mut tx = conn.transaction()?;
    let mut stmt = tx.prepare(&query)?;
    
    // Process records in a streaming fashion
    for result in rdr.records() {
        match result {
            Ok(record) => {
                // Apply extension filter if specified
                if let Some(exts) = &extensions {
                    // Get the extension directly from the Extension column
                    let file_ext = if let Some(ext_index) = headers.iter().position(|h| h.eq_ignore_ascii_case("Extension")) {
                        record.get(ext_index).unwrap_or("").trim().to_lowercase()
                    } else {
                        // Fallback to index 2 if Extension column not found
                        record.get(2).unwrap_or("").trim().to_lowercase()
                    };
                    
                    // Skip if extension doesn't match
                    let normalized_ext = file_ext.trim_start_matches('.');
                    if !exts.iter().any(|ext| {
                        let ext_normalized = ext.trim_start_matches('.');
                        ext_normalized.eq_ignore_ascii_case(normalized_ext)
                    }) {
                        continue;
                    }
                }
                
                batch_records.push(record);
                
                // Process a batch when it reaches the batch size
                if batch_records.len() >= BATCH_SIZE {
                    let before_unique = processed_hashes.len();
                    
                    process_batch(
                        &mut batch_records,
                        &mut stmt,
                        &mut known_writer,
                        &mut unknown_writer,
                        &mut known_count,
                        &mut unknown_count,
                        &mut empty_hash_count,
                        &mut error_count,
&mut processed_hashes, // Pass the mutable HashSet reference
                    )?;
                    
                    let new_unique = processed_hashes.len() - before_unique;
                    unique_processed += new_unique as u64;
                    records_processed += batch_records.len() as u64;
                    batch_records.clear();
                    
                    // Update progress based on unique hashes processed
                    pb.set_position(unique_processed);
                    
                    // Commit transaction periodically
                    if records_processed % (BATCH_SIZE as u64 * COMMIT_INTERVAL as u64) == 0 {
                        // Commit current transaction and start a new one
                        drop(stmt);
                        tx.commit()?;
                        tx = conn.transaction()?;
                        stmt = tx.prepare(&query)?;
                        
                        // Flush writers
                        known_writer.flush()?;
                        unknown_writer.flush()?;
                    }
                    
                    // Update status bar with statistics
                    if records_processed % PROGRESS_UPDATE_INTERVAL == 0 || 
                       last_update_time.elapsed() >= Duration::from_secs(1) {
                        let elapsed = last_update_time.elapsed().as_secs_f64();
                        let records_since_last = records_processed - last_update_count;
                        
                        if elapsed >= 0.5 { // Only update if at least half a second has passed
                            let speed = records_since_last as f64 / elapsed;
                            status_bar.set_message(format!("{} ({:.1}%)", 
                                unknown_count,
                                if unique_processed > 0 { (unknown_count as f64 / unique_processed as f64) * 100.0 } else { 0.0 }));
                            status_bar.set_prefix(format!("{} ({:.1}%)", 
                                known_count,
                                if unique_processed > 0 { (known_count as f64 / unique_processed as f64) * 100.0 } else { 0.0 }));
                            status_bar.set_position(unique_processed);
                            status_bar.set_length(unique_hash_count);
                            
                            pb.set_message(format!("Processing at {:.0} records/sec", speed));
                            
                            last_update_time = Instant::now();
                            last_update_count = records_processed;
                        }
                    }
                }
            },
            Err(e) => {
                eprintln!("Error reading CSV record: {}", e);
                error_count += 1;
                if error_count > 100 {
                    return Err(Box::new(e));
                }
            }
        }
    }
    
    // Process any remaining records in the last batch
    if !batch_records.is_empty() {
        let before_unique = processed_hashes.len();
        
        process_batch(
            &mut batch_records,
            &mut stmt,
            &mut known_writer,
            &mut unknown_writer,
            &mut known_count,
            &mut unknown_count,
            &mut empty_hash_count,
            &mut error_count,
            &mut processed_hashes,

        )?;
        
        let new_unique = processed_hashes.len() - before_unique;
        unique_processed += new_unique as u64;
        records_processed += batch_records.len() as u64;
        pb.set_position(unique_processed);
    }
    
    // Commit the final transaction
    drop(stmt);
    tx.commit()?;
    // Finish progress bars
    pb.finish_with_message("Processing complete!");
    status_bar.finish_and_clear();

    // Ensure all data is written
    known_writer.flush()?;
    unknown_writer.flush()?;

    let duration = start_time.elapsed();
    let records_per_second = records_processed as f64 / duration.as_secs_f64();
    
    println!("\nDetailed Summary:");
    println!("  Total records processed: {}", records_processed);
    println!("  Unique hash values: {} ({:.1}%)", 
        processed_hashes.len(),
        (processed_hashes.len() as f64 / records_processed as f64) * 100.0);
    println!("  Known software: {} ({:.1}%)", 
        known_count, 
        (known_count as f64 / processed_hashes.len() as f64) * 100.0);
    println!("  Unknown software: {} ({:.1}%)", 
        unknown_count, 
        (unknown_count as f64 / processed_hashes.len() as f64) * 100.0);
    println!("  Records with empty hashes: {} ({:.1}%)", 
        empty_hash_count,
        (empty_hash_count as f64 / records_processed as f64) * 100.0);
    println!("  Duplicate hash values: {} ({:.1}%)",
        records_processed - processed_hashes.len() as u64,
        ((records_processed - processed_hashes.len() as u64) as f64 / records_processed as f64) * 100.0);
    if error_count > 0 {
        println!("  Query errors encountered: {}", error_count);
    }
    println!("  Processing speed: {:.0} records/second", records_per_second);
    println!("  Total processing time: {:.2} seconds", duration.as_secs_f64());

    Ok(())
}