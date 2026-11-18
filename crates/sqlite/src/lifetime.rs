//! `$result = null; $db->exec('DETACH extra')` finalizes the cursor before DETACH.

use crate::Database;
use std::sync::OnceLock;
use tokio::task::JoinHandle;

pub(crate) struct CursorTask {
    pub database: Database,
    pub task: OnceLock<JoinHandle<()>>,
}

impl Drop for CursorTask {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            self.database.finish_cursor(task);
        }
    }
}
