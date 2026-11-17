//! PDO supplies database reset; shared accounting retains permits until reset finishes.

use crate::drivers::Driver;
use crate::errors::DbError;

pub type Bucket = phera_io_pool::Bucket<Driver>;

impl phera_io_pool::Connection for Driver {
    type Error = DbError;
    fn reusable(&self) -> bool {
        Driver::reusable(self)
    }
    async fn reset(&mut self) -> Result<(), DbError> {
        Driver::reset(self).await
    }
    async fn close(self) {
        Driver::close(self).await;
    }
}
