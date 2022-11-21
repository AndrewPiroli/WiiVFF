use crate::*;

fn open() -> Result<std::fs::File> {
    Ok(std::fs::File::open("test_data/cdb.vff")?)
}

#[test]
pub fn debug_vff() -> Result<()> {
    let f = open()?;
    let (vff, _) = VFF::new(f)?;
    println!("{vff:?}");
    Ok(())
}
    
#[test]
pub fn debug_root_dir() -> Result<()> {
    let f = open()?;
    let (_, root_dir) = VFF::new(f)?;
    println!("{root_dir:?}");
    Ok(())
}

#[test]
pub fn ls_root_dir() -> Result<()> {
    let f = open()?;
    let (_, root_dir) = VFF::new(f)?;
    let a = root_dir.ls(None, None)?;
    dbg!(a);
    Ok(())
}

#[test]
pub fn dump_root() -> Result<()> {
    let f = open()?;
    let (_, root_dir) = VFF::new(f)?;
    root_dir.ls(None, Some("/tmp".to_owned()))?;
    Ok(())
}
