use std::path::PathBuf;
use workflow_core::dirs::home_dir;

pub struct Folders {
    pub home: PathBuf,
    pub data: PathBuf,
    pub logs: PathBuf,
    pub kaspa: PathBuf,
    pub snapshots: PathBuf,
    pub sync: PathBuf,
}

impl Default for Folders {
    fn default() -> Self {
        let home = home_dir().unwrap();
        let data = home.join(".krc721");
        let logs = data.join("logs");
        let kaspa = data.join("kaspa");
        let snapshots = data.join("snapshots");
        let sync = snapshots.join("sync");

        [&logs, &kaspa, &snapshots, &sync]
            .into_iter()
            .for_each(|dir| {
                if let Err(err) = std::fs::create_dir_all(dir) {
                    panic!("Unable to create directory: `{err}`");
                }
            });

        Self {
            home,
            data,
            logs,
            kaspa,
            snapshots,
            sync,
        }
    }
}
