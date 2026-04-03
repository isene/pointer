use std::path::PathBuf;

#[derive(Clone)]
pub enum UndoOp {
    Delete { paths: Vec<PathBuf>, trash_paths: Vec<(PathBuf, PathBuf)> },
    Move { moves: Vec<(PathBuf, PathBuf)> },
    Rename { old: PathBuf, new: PathBuf },
    Copy { created: Vec<PathBuf> },
    Link { created: Vec<PathBuf> },
    BulkRename { renames: Vec<(PathBuf, PathBuf)> },
    Permissions { path: PathBuf, old_mode: u32 },
}
