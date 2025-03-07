# NSRL Filter

A high-performance tool for filtering file lists against the National Software Reference Library (NSRL) database to identify known and unknown software.

## Overview

NSRL Filter is a Rust-based utility that compares file hash values (SHA-1) from a CSV file against the NSRL database to separate known software from unknown files. This tool is particularly useful for digital forensics and malware analysis workflows.

## Features

- **High Performance**: Uses parallel processing via Rayon for fast SHA-1 lookups
- **Automatic Indexing**: Creates and uses an indexed version of the database for faster lookups
- **Progress Visualization**: Real-time progress bars and status updates
- **Memory Efficient**: Processes large datasets with minimal memory footprint
- **Flexible Input/Output**: Works with standard CSV file formats

## Installation

### Prerequisites

- Rust and Cargo (latest stable version recommended)

### Building from Source

```bash
# Clone the repository
git clone https://github.com/yourusername/nsrl-filter.git
cd nsrl-filter

# Build in release mode for optimal performance
cargo build --release
```

The compiled binary will be available at `target/release/nsrl-filter`.

## Usage

```bash
# Basic usage with default paths
nsrl-filter

# Specify custom database and file list paths
nsrl-filter path/to/nsrl.db path/to/filelist.csv
```

### Input Format

The tool expects a CSV file with headers containing at least a SHA1 column. The format should match the standard forensic tool export format with fields like Name, Path, SHA1, etc.

### Output

The tool generates two CSV files in the same directory as the database:
- `[filename]_known_software.csv`: Files that match entries in the NSRL database
- `[filename]_unknown_software.csv`: Files that don't match any entry in the NSRL database

## Performance Notes

- The first run will create an indexed copy of the database which may take some time but significantly speeds up subsequent runs
- Performance is heavily dependent on database size and the number of unique SHA-1 values
- The parallel processing feature automatically scales to use available CPU cores

## Database Schema

CREATE TABLE FILE ( 
sha256     VARCHAR NOT NULL, 
sha1       VARCHAR NOT NULL, 
md5        VARCHAR NOT NULL, 
crc32      
VARCHAR NOT NULL, 
file_name  VARCHAR NOT NULL, 
file_size  INTEGER NOT NULL, 
package_id INTEGER NOT NULL, 
CONSTRAINT PK_FILE__FILE PRIMARY KEY (sha256, sha1, md5, file_name, file_size, package_id) 
); 
CREATE TABLE MFG ( 
manufacturer_id INTEGER NOT NULL, 
name            VARCHAR NOT NULL, 
CONSTRAINT PK_MFG__MFG_ID PRIMARY KEY (manufacturer_id) 
); 
CREATE TABLE OS ( 
operating_system_id INTEGER NOT NULL, 
name                VARCHAR NOT NULL,   
version             
VARCHAR NOT NULL, 
manufacturer_id     INTEGER NOT NULL, 
CONSTRAINT PK_OS__OS_ID PRIMARY KEY (operating_system_id, manufacturer_id) 
); 
CREATE TABLE PKG ( 
package_id          INTEGER NOT NULL, 
name                VARCHAR NOT NULL, 
version             VARCHAR NOT NULL, 
operating_system_id INTEGER NOT NULL, 
manufacturer_id     INTEGER NOT NULL, 
language            VARCHAR NOT NULL, 
application_type    VARCHAR NOT NULL, 
CONSTRAINT PK_PGK__PKG_ID PRIMARY KEY (package_id, operating_system_id, manufacturer_id, language, 
application_type) 
); 
CREATE TABLE VERSION ( 
version      VARCHAR UNIQUE NOT NULL, 
build_set    VARCHAR NOT NULL, 
build_date   TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL, 
release_date TIMESTAMP NOT NULL, 
description  VARCHAR NOT NULL, 
CONSTRAINT PK_VERSION__VERSION PRIMARY KEY (version) 
); 
CREATE VIEW DISTINCT_HASH AS 
SELECT DISTINCT 
sha256, 
sha1, 
md5, 
crc32 
FROM 
FILE 
/* DISTINCT_HASH(sha256,sha1,md5,crc32) */;

## License

[MIT License](LICENSE)

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.