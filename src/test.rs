use crate::*;
use std::io::Read;

fn open() -> Result<std::fs::File> {
    Ok(std::fs::File::open("test_data/cdb.vff")?)
}

#[test]
pub fn vff_header() -> Result<()> {
    let f = open()?;
    let (vff, _) = VFF::new(f)?;
    let header = &vff.borrow().header;
    assert_eq!(header.volume_size, 0x1400000);
    assert_eq!(header.cluster_size, 0x200);
    assert_eq!(header.cluster_count, 0xa000);
    Ok(())
}

#[test]
pub fn ls_root_dir() -> Result<()> {
    let f = open()?;
    let (_, root_dir) = VFF::new(f)?;
    let root_dir_contents = root_dir.ls(false)?;
    assert_eq!(root_dir_contents.len(), 2);
    assert!(root_dir_contents.contains(&"/CDB~1.CON [0x0004]".to_owned()));
    assert!(root_dir_contents
        .contains(&"/2022/10/15/21/44/HAEA_#1/LOG/2B06C4C3.000 [0x0ca0]".to_owned()));
    Ok(())
}

#[test]
pub fn dump_root() -> Result<()> {
    let f = open()?;
    let mut test_dir = std::env::temp_dir();
    test_dir.push("WiiVFF-tests");
    if test_dir.exists() {
        std::fs::remove_dir_all(&test_dir)?;
    }
    let (_, root_dir) = VFF::new(f)?;
    root_dir.dump(test_dir.clone(), false)?;
    let file1 = {
        let mut temp = test_dir.clone();
        temp.push("CDB~1.CON");
        temp
    };
    let mut cdb_file: Vec<u8> = Vec::with_capacity(0x4);
    let cdb_file_size = File::open(file1)?.read_to_end(&mut cdb_file)?;
    assert_eq!(cdb_file_size, 0x4);
    assert_eq!(cdb_file, vec![0; 4]);

    let file2_compare = "test_data/compare.bin".to_owned();
    let mut file2_compare_data: Vec<u8> = Vec::with_capacity(0xca0);
    let file2_compare_size = File::open(file2_compare)?.read_to_end(&mut file2_compare_data)?;

    let file2 = {
        let mut temp = test_dir.clone();
        temp.push("2022/10/15/21/44/HAEA_#1/LOG/2B06C4C3.000");
        temp
    };
    dbg!(&file2);
    let mut playlog_file: Vec<u8> = Vec::with_capacity(0xca0);
    let playlog_file_size = File::open(file2)?.read_to_end(&mut playlog_file)?;
    assert_eq!(playlog_file_size, 0xca0);
    assert_eq!(file2_compare_size, playlog_file_size);
    assert_eq!(playlog_file, file2_compare_data);

    if std::path::Path::new(&test_dir).exists() {
        std::fs::remove_dir_all(&test_dir)?;
    }
    Ok(())
}

#[test]
pub fn check_file_size_vs_header() -> Result<()> {
    let mut f = open()?;
    f.seek(io::SeekFrom::End(0))?;
    let expected_size = f.stream_position()? as u32;
    f.seek(io::SeekFrom::Start(0))?;
    let (vff, _) = VFF::new(f)?;
    let header = &vff.borrow().header;
    assert_eq!(header.volume_size, expected_size);
    Ok(())
}

#[test]
pub fn build_header_fields() -> Result<()> {
    let tmp = temp_dir_unique("WiiVFF-build-header");
    let vff_path = tmp.join("out.vff");

    let src = tmp.join("src");
    std::fs::create_dir_all(&src)?;
    std::fs::write(src.join("HELLO.TXT"), b"hello")?;

    build_vff(&src, &vff_path, BUILD_DEFAULT_VOLUME_SIZE)?;

    let meta = std::fs::metadata(&vff_path)?;
    assert_eq!(meta.len(), BUILD_DEFAULT_VOLUME_SIZE as u64);

    let f = File::open(&vff_path)?;
    let (vff, _) = VFF::new(f)?;
    let header = &vff.borrow().header;
    assert_eq!(header.volume_size, BUILD_DEFAULT_VOLUME_SIZE);
    assert_eq!(header.cluster_size, BUILD_CLUSTER_SIZE);
    assert_eq!(
        header.cluster_count,
        BUILD_DEFAULT_VOLUME_SIZE / BUILD_CLUSTER_SIZE as u32
    );
    cleanup(tmp);
    Ok(())
}

#[test]
pub fn build_single_file_ls() -> Result<()> {
    let tmp = temp_dir_unique("WiiVFF-build-single");
    let vff_path = tmp.join("out.vff");
    let src = tmp.join("src");
    std::fs::create_dir_all(&src)?;
    std::fs::write(src.join("HELLO.TXT"), b"hello world")?;

    build_vff(&src, &vff_path, BUILD_DEFAULT_VOLUME_SIZE)?;

    let f = File::open(&vff_path)?;
    let (_, root) = VFF::new(f)?;
    let listing = root.ls(false)?;
    assert_eq!(listing.len(), 1);
    assert!(
        listing[0].contains("HELLO.TXT"),
        "Expected HELLO.TXT in listing, got: {:?}",
        listing
    );
    assert!(
        listing[0].contains("0x000b"),
        "Expected size 0x000b (11) in listing, got: {:?}",
        listing
    );
    cleanup(tmp);
    Ok(())
}

#[test]
pub fn build_round_trip_cdb() -> Result<()> {
    let tmp = temp_dir_unique("WiiVFF-build-roundtrip");
    let dump_dir = tmp.join("dump");
    let rebuilt_vff = tmp.join("rebuilt.vff");

    let f = open()?;
    let (_, root) = VFF::new(f)?;
    let original_listing = root.ls(false)?;
    root.dump(dump_dir.clone(), false)?;

    build_vff(&dump_dir, &rebuilt_vff, BUILD_DEFAULT_VOLUME_SIZE)?;

    let f2 = File::open(&rebuilt_vff)?;
    let (vff2, root2) = VFF::new(f2)?;
    assert_eq!(vff2.borrow().header.volume_size, BUILD_DEFAULT_VOLUME_SIZE);

    let rebuilt_listing = root2.ls(false)?;
    assert_eq!(
        original_listing.len(),
        rebuilt_listing.len(),
        "Listing length mismatch.\nOriginal: {original_listing:?}\nRebuilt: {rebuilt_listing:?}"
    );
    for entry in &original_listing {
        assert!(
            rebuilt_listing.iter().any(|r| r == entry),
            "Entry '{entry}' from original not found in rebuilt listing.\nRebuilt: {rebuilt_listing:?}"
        );
    }

    let cdb_original: Vec<u8> = {
        let mut v = Vec::new();
        File::open("test_data/compare.bin")?.read_to_end(&mut v)?;
        v
    };
    let cdb_rebuilt: Vec<u8> = {
        let mut path = dump_dir.clone();
        path.push("2022/10/15/21/44/HAEA_#1/LOG/2B06C4C3.000");
        let mut v = Vec::new();
        File::open(&path)?.read_to_end(&mut v)?;
        v
    };
    assert_eq!(cdb_original, cdb_rebuilt, "File content mismatch after round-trip");

    cleanup(tmp);
    Ok(())
}

#[test]
pub fn build_nested_dirs() -> Result<()> {
    let tmp = temp_dir_unique("WiiVFF-build-nested");
    let vff_path = tmp.join("out.vff");
    let src = tmp.join("src");

    // Build: src/A/B/FILE.BIN
    let nested = src.join("A").join("B");
    std::fs::create_dir_all(&nested)?;
    std::fs::write(nested.join("FILE.BIN"), &[0xde, 0xad, 0xbe, 0xef])?;

    build_vff(&src, &vff_path, BUILD_DEFAULT_VOLUME_SIZE)?;

    let f = File::open(&vff_path)?;
    let (_, root) = VFF::new(f)?;
    let listing = root.ls(false)?;
    assert_eq!(listing.len(), 1, "Expected 1 entry, got: {listing:?}");
    let entry = &listing[0];
    assert!(entry.contains("A"), "Expected dir 'A' in path: {entry}");
    assert!(entry.contains("B"), "Expected dir 'B' in path: {entry}");
    assert!(entry.contains("FILE.BIN"), "Expected FILE.BIN: {entry}");
    assert!(entry.contains("0x0004"), "Expected size 4: {entry}");

    cleanup(tmp);
    Ok(())
}

// ugh
fn temp_dir_unique(name: &str) -> std::path::PathBuf {
    let mut p = std::env::temp_dir();
    p.push(name);
    if p.exists() {
        let _ = std::fs::remove_dir_all(&p);
    }
    std::fs::create_dir_all(&p).expect("create temp dir");
    p
}

fn cleanup(p: std::path::PathBuf) {
    if p.exists() {
        let _ = std::fs::remove_dir_all(p);
    }
}
