use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

// Snapshot bytes, ordinary metadata and link identities without following links
// or opening non-regular files such as diagnostic FIFO fixtures.
pub fn tree(root: &Path) -> BTreeMap<PathBuf, (u32, Vec<u8>)> {
    fn visit(path: &Path, entries: &mut BTreeMap<PathBuf, (u32, Vec<u8>)>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        let bytes = if metadata.is_file() {
            fs::read(path).unwrap()
        } else if metadata.is_symlink() {
            fs::read_link(path)
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
                .to_vec()
        } else {
            Vec::new()
        };
        entries.insert(path.to_owned(), (metadata.permissions().mode(), bytes));
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(&entry.unwrap().path(), entries);
            }
        }
    }
    let mut entries = BTreeMap::new();
    visit(root, &mut entries);
    entries
}
