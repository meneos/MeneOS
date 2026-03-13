use std::path::{Path, PathBuf};

use crate::error::{Result, XtaskError};

pub struct Workspace {
    pub root_dir: PathBuf,
    pub arceos_dir: PathBuf,
    pub app_dir: PathBuf,
}

impl Workspace {
    pub fn discover(app: &str) -> Result<Self> {
        let root = workspace_root()?;
        let arceos_dir = root.join("arceos");
        ensure_file(&arceos_dir.join("Makefile"), "ArceOS Makefile")?;

        Ok(Self {
            root_dir: root.clone(),
            arceos_dir,
            app_dir: root.join(app),
        })
    }
}

fn workspace_root() -> Result<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| XtaskError::Message("failed to locate workspace root".to_string()))
}

fn ensure_file(path: &Path, name: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        Err(XtaskError::Message(format!(
            "{name} not found: {}",
            path.display()
        )))
    }
}
