use std::path::{Path, PathBuf};

pub fn workspace_files(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in root.read_dir()? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            files.push(entry.path());
        }
    }
    files.sort();
    Ok(files)
}
