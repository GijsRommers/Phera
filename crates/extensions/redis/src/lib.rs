//! Async phpredis: `$r = new Redis(); $r->connect("localhost"); $r->get("key");`.
mod arguments;
mod auth;
mod commands;
mod connection;
mod constructor;
mod dispatch;
mod options;
mod pool;
mod pubsub;
mod registration;
mod reply;
mod scan;
mod transaction;

use phera_extension_api::{Extension, Registry};
pub struct Redis;
impl Extension for Redis {
    fn name(&self) -> &'static str {
        "redis"
    }
    fn register(&self, registry: &mut Registry) {
        registration::register(registry);
    }
}
