use tesla::FullVehicleData;
use crate::config::Config;
use crate::sink::sqlite::SqliteSink;

mod sqlite;

pub trait Sink {
    fn save(&self, vehicle_data: &FullVehicleData);
    fn destroy(&self);
}

pub fn new_sink(config: Config) -> Option<Box<dyn Sink>> {
    if config.sqlite.is_some() {
        return Some(SqliteSink::new(config));
    }

    None
}