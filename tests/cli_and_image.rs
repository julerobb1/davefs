//! Integration tests: library open/scan + CLI against the synthetic fixture.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use davefs::fixture::{
    write_partition_fixture, write_whole_disk_fixture, FIXTURE_AV_REQ_HD_OFFSET,
    FIXTURE_EFS_MISC_OFFSET, FIXTURE_ES_RESERVED_OFFSET, MBR_FIXTURE_PARTITION_OFFSET,
    REFERENCE_SECTOR_COUNT,
};
use davefs::{DaveImage, ImageKind, Superblock, VENDOR_CSTR};

fn tmp_path(name: &str) -> PathBuf {
    let dir = std::env::var_os("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("davefs-tests"));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("{}-{name}", std::process::id()));
    let _ = fs::remove_file(&path);
    path
}

fn dave_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_dave"))
}

#[test]
fn open_partition_fixture_and_list_substores() {
    let path = tmp_path("partition.dave");
    {
        let mut f = fs::File::create(&path).unwrap();
        write_partition_fixture(&mut f).unwrap();
    }

    let mut img = DaveImage::open(&path, None).unwrap();
    assert_eq!(img.kind(), ImageKind::PartitionImage);
    assert_eq!(img.partition_offset(), 0);
    assert_eq!(img.superblock().sector_count, REFERENCE_SECTOR_COUNT);

    let stores = img.substores(false).unwrap();
    let names: Vec<_> = stores.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["AV_REQ_HD", "ES_RESERVED", "EFSMisc"]);
    assert_eq!(stores[0].offset, FIXTURE_AV_REQ_HD_OFFSET);
    assert_eq!(stores[1].offset, FIXTURE_ES_RESERVED_OFFSET);
    assert_eq!(stores[2].offset, FIXTURE_EFS_MISC_OFFSET);
    assert_eq!(stores[0].block_size_candidate, Some(0x2000));
    assert!(stores[0].mpeg_pes_e0_after_header);
    assert!(!stores[1].mpeg_pes_e0_after_header);
}

#[test]
fn open_whole_disk_fixture_finds_mbr_linux_partition() {
    let path = tmp_path("whole.disk");
    {
        let mut f = fs::File::create(&path).unwrap();
        write_whole_disk_fixture(&mut f).unwrap();
    }

    let mut img = DaveImage::open(&path, None).unwrap();
    assert_eq!(img.kind(), ImageKind::WholeDisk);
    assert_eq!(img.partition_offset(), MBR_FIXTURE_PARTITION_OFFSET);
    let stores = img.substores(true).unwrap();
    assert_eq!(stores.len(), 1);
    assert_eq!(stores[0].name, "AV_REQ_HD");
}

#[test]
fn explicit_offset_opens_embedded_partition() {
    let path = tmp_path("explicit.disk");
    {
        let mut f = fs::File::create(&path).unwrap();
        write_whole_disk_fixture(&mut f).unwrap();
    }
    let img = DaveImage::open(&path, Some(MBR_FIXTURE_PARTITION_OFFSET)).unwrap();
    assert_eq!(img.kind(), ImageKind::ExplicitOffset);
    assert_eq!(img.superblock().magic, "DaveSystemDisk");
}

#[test]
fn cli_info_and_substores_against_fixture() {
    let path = tmp_path("cli.dave");
    {
        let mut f = fs::File::create(&path).unwrap();
        write_partition_fixture(&mut f).unwrap();
    }

    let info = Command::new(dave_bin())
        .args(["info", path.to_str().unwrap()])
        .output()
        .expect("run dave info");
    assert!(
        info.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&info.stderr)
    );
    let stdout = String::from_utf8_lossy(&info.stdout);
    assert!(stdout.contains("DaveSystemDisk"), "{stdout}");
    assert!(stdout.contains("Echostar Technologies Corp."), "{stdout}");
    assert!(stdout.contains("966866850"), "{stdout}");
    assert!(stdout.contains("read-only"), "{stdout}");

    let subs = Command::new(dave_bin())
        .args(["substores", path.to_str().unwrap()])
        .output()
        .expect("run dave substores");
    assert!(
        subs.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&subs.stderr)
    );
    let stdout = String::from_utf8_lossy(&subs.stdout);
    assert!(stdout.contains("AV_REQ_HD"), "{stdout}");
    assert!(stdout.contains("ES_RESERVED"), "{stdout}");
    assert!(stdout.contains("EFSMisc"), "{stdout}");
    assert!(stdout.contains("Recordings"), "{stdout}");
}

#[test]
fn cli_hexdump_shows_vendor_string() {
    let path = tmp_path("hex.dave");
    {
        let mut f = fs::File::create(&path).unwrap();
        write_partition_fixture(&mut f).unwrap();
    }

    let out = Command::new(dave_bin())
        .args([
            "hexdump",
            path.to_str().unwrap(),
            "--offset",
            "0",
            "--length",
            "64",
        ])
        .output()
        .expect("run dave hexdump");
    assert!(
        out.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Echostar"), "{stdout}");
    assert!(stdout.contains("00000000"), "{stdout}");
}

#[test]
fn cli_hexdump_at_substore_offset() {
    let path = tmp_path("hex2.dave");
    {
        let mut f = fs::File::create(&path).unwrap();
        write_partition_fixture(&mut f).unwrap();
    }

    let out = Command::new(dave_bin())
        .args([
            "hexdump",
            path.to_str().unwrap(),
            "--offset",
            "0x10000",
            "--length",
            "32",
        ])
        .output()
        .expect("run dave hexdump");
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("Long Live Dave!"), "{stdout}");
}

#[test]
fn rejects_non_dave_file() {
    let path = tmp_path("random.bin");
    fs::write(&path, [0u8; 1024]).unwrap();
    let err = DaveImage::open(&path, None).unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("not a DaveSystemDisk"), "{msg}");
}

#[test]
fn superblock_helper_matches_file_bytes() {
    let path = tmp_path("raw.dave");
    {
        let mut f = fs::File::create(&path).unwrap();
        write_partition_fixture(&mut f).unwrap();
    }
    let bytes = fs::read(&path).unwrap();
    assert!(bytes.starts_with(VENDOR_CSTR));
    let sb = Superblock::parse(&bytes[..128]).unwrap();
    assert_eq!(sb.sector_count, REFERENCE_SECTOR_COUNT);
}
