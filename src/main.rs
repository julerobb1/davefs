use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use davefs::{hexdump, parse_bytes, DaveImage, ImageKind, SECTOR_SIZE};

#[derive(Parser, Debug)]
#[command(
    name = "dave",
    version,
    about = "Read-only research CLI for Echostar/Dish DaveSystemDisk (E*FS) images",
    long_about = "Userspace, read-only parser for DaveSystemDisk partitions dumped from a \
decommissioned DVR. This is not a kernel driver and must not be used to write \
anything a live set-top box would execute."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print the Dave superblock and how the partition was located
    Info {
        /// Partition image or whole-disk image
        path: PathBuf,
        /// Byte offset of the Dave partition inside the file (decimal, 0x…, or K/M/G)
        #[arg(long, value_parser = clap_bytes)]
        partition_offset: Option<u64>,
    },
    /// List inner substores (`Long Live Dave!` + name)
    Substores {
        path: PathBuf,
        #[arg(long, value_parser = clap_bytes)]
        partition_offset: Option<u64>,
        /// Scan the entire readable partition instead of hints + first 128 MiB
        #[arg(long)]
        scan: bool,
    },
    /// Hex + ASCII dump from an absolute file offset
    Hexdump {
        path: PathBuf,
        /// Absolute byte offset from the start of the file
        #[arg(long, value_parser = clap_bytes)]
        offset: u64,
        /// Bytes to read (default 256)
        #[arg(long, value_parser = clap_bytes, default_value = "256")]
        length: u64,
    },
}

fn clap_bytes(s: &str) -> Result<u64, String> {
    parse_bytes(s).map_err(|e| e.to_string())
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("dave: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> davefs::Result<()> {
    match cli.command {
        Command::Info {
            path,
            partition_offset,
        } => cmd_info(&path, partition_offset),
        Command::Substores {
            path,
            partition_offset,
            scan,
        } => cmd_substores(&path, partition_offset, scan),
        Command::Hexdump {
            path,
            offset,
            length,
        } => cmd_hexdump(&path, offset, length),
    }
}

fn cmd_info(path: &std::path::Path, partition_offset: Option<u64>) -> davefs::Result<()> {
    let img = DaveImage::open(path, partition_offset)?;
    let sb = img.superblock();
    let declared = sb.declared_size_bytes();
    let readable = img.readable_partition_len();

    println!("source:              {}", img.path().display());
    println!("kind:                {}", kind_label(img.kind()));
    println!(
        "partition_offset:    {} ({:#x})",
        img.partition_offset(),
        img.partition_offset()
    );
    println!(
        "file_size:           {} ({})",
        img.file_len(),
        human_bytes(img.file_len())
    );
    println!();
    println!("Dave superblock");
    println!("  vendor:            {}", sb.vendor);
    println!("  magic:             {}", sb.magic);
    println!(
        "  sector_count @ +{:#x}: {} ({}-byte sectors)",
        davefs::SUPERBLOCK_SECTOR_COUNT_OFFSET,
        sb.sector_count,
        SECTOR_SIZE
    );
    println!(
        "  declared_size:     {} ({})",
        declared,
        human_bytes(declared)
    );
    println!(
        "  readable_in_file:  {} ({})",
        readable,
        human_bytes(readable)
    );
    if readable < declared {
        println!(
            "  note:              image is shorter than the declared partition size \
             (truncated dump or synthetic fixture)"
        );
    }
    println!();
    println!("safety: read-only open; this tool never writes the image.");
    Ok(())
}

fn cmd_substores(
    path: &std::path::Path,
    partition_offset: Option<u64>,
    scan: bool,
) -> davefs::Result<()> {
    let mut img = DaveImage::open(path, partition_offset)?;
    let stores = img.substores(scan)?;

    println!("source:           {}", img.path().display());
    println!(
        "partition_offset: {} ({:#x})",
        img.partition_offset(),
        img.partition_offset()
    );
    if stores.is_empty() {
        println!();
        println!(
            "no substores found (magic `Long Live Dave!`). \
             Try --scan on a full dump, or confirm the partition offset."
        );
        return Ok(());
    }

    println!("substores:        {}", stores.len());
    println!();
    println!(
        "{:<4} {:<18} {:<14} {:<12} {}",
        "#", "offset", "name", "role", "notes"
    );
    for (i, ss) in stores.iter().enumerate() {
        let mut notes = Vec::new();
        if let (Some(v), Some(at)) = (ss.block_size_candidate, ss.block_size_candidate_at) {
            notes.push(format!(
                "block_size_candidate={v} (0x{v:x}) at header+{at:#x} [hypothesis]"
            ));
        }
        if ss.mpeg_pes_e0_after_header {
            notes.push("MPEG PES 0xE0 after header".into());
        }
        if let Some(n) = &ss.note {
            notes.push(n.clone());
        }
        println!(
            "{:<4} {:#018x} {:<14} {:<12} {}",
            i,
            ss.offset,
            ss.name,
            ss.role.as_str(),
            notes.join("; ")
        );
    }
    println!();
    println!(
        "offsets are relative to the Dave partition start. \
         Allocation tables / object index: not decoded (TODO)."
    );
    Ok(())
}

fn cmd_hexdump(path: &std::path::Path, offset: u64, length: u64) -> davefs::Result<()> {
    use std::fs::OpenOptions;
    use std::io::{Read, Seek, SeekFrom};

    if length == 0 {
        return Ok(());
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(false)
        .create(false)
        .open(path)
        .map_err(|e| davefs::Error::io(Some(path.to_path_buf()), e))?;
    let file_len = file
        .metadata()
        .map_err(|e| davefs::Error::io(Some(path.to_path_buf()), e))?
        .len();
    if offset >= file_len {
        return Err(davefs::Error::OffsetOutOfRange {
            offset,
            length: file_len,
        });
    }
    let want = (length as usize).min((file_len - offset) as usize);
    let mut buf = vec![0u8; want];
    file.seek(SeekFrom::Start(offset))?;
    let n = file.read(&mut buf)?;
    buf.truncate(n);
    print!("{}", hexdump(&buf, offset));
    Ok(())
}

fn kind_label(kind: ImageKind) -> &'static str {
    match kind {
        ImageKind::PartitionImage => "dave-partition",
        ImageKind::WholeDisk => "whole-disk (MBR)",
        ImageKind::ExplicitOffset => "explicit --partition-offset",
    }
}

fn human_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i + 1 < UNITS.len() {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{n} B")
    } else {
        format!("{v:.2} {}", UNITS[i])
    }
}
