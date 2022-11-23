use crate::*;

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
    assert!(root_dir_contents.contains(&"/2022/10/15/21/44/HAEA_#1/LOG/2B06C4C3.000 [0x0ca0]".to_owned()));
    Ok(())
}

#[test]
pub fn dump_root() -> Result<()> {
    let f = open()?;
    let temp_dir = std::env::temp_dir().to_string_lossy().into_owned() + "/WiiVFF-tests";
    if std::path::Path::new(&temp_dir).exists() {
        std::fs::remove_dir_all(&temp_dir)?;
    }
    let (_, root_dir) = VFF::new(f)?;
    root_dir.dump(temp_dir.clone(), false)?;
    let file1 = temp_dir.clone() + "/CDB~1.CON";
    let mut cdb_file: Vec<u8> = Vec::with_capacity(0x4);
    let cdb_file_size = File::open(file1)?.read_to_end(&mut cdb_file)?;
    assert_eq!(cdb_file_size, 0x4);
    assert_eq!(cdb_file, vec![0;4]);

    let file2_compare = "test_data/compare.bin".to_owned();
    let mut file2_compare_data: Vec<u8> = Vec::with_capacity(0xca0);
    let file2_compare_size = File::open(file2_compare)?.read_to_end(&mut file2_compare_data)?;

    let file2 = temp_dir.clone() + "/2022/10/15/21/44/HAEA_#1/LOG/2B06C4C3.000";
    let mut playlog_file: Vec<u8> = Vec::with_capacity(0xca0);
    let playlog_file_size = File::open(file2)?.read_to_end(&mut playlog_file)?;
    assert_eq!(playlog_file_size, 0xca0);
    assert_eq!(file2_compare_size, playlog_file_size);
    assert_eq!(playlog_file, file2_compare_data);

    if std::path::Path::new(&temp_dir).exists() {
        std::fs::remove_dir_all(&temp_dir)?;
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
