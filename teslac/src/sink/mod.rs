use tesla::FullVehicleData;
use crate::config::Config;
#[cfg(feature = "sqlite")]
use crate::sink::sqlite::SqliteSink;

#[cfg(feature = "sqlite")]
mod sqlite;

pub trait Sink {
    fn save(&self, vehicle_data: &FullVehicleData);
    fn destroy(&self);
}

pub fn new_sink(config: Config) -> Option<Box<dyn Sink>> {
    #[cfg(feature = "sqlite")]
    if config.sqlite.is_some() {
        return Some(SqliteSink::new(config));
    }

    None
}