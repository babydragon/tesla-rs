use tesla::FullVehicleData;
use crate::config::Config;

#[cfg(feature = "sqlite")]
mod sqlite;
#[cfg(feature = "mqtt")]
mod mqtt;

pub trait Sink {
    fn save(&mut self, vehicle_data: &FullVehicleData);
    fn destroy(&mut self);
}

pub fn new_sink(config: Config) -> Option<Box<dyn Sink>> {
    #[cfg(feature = "sqlite")]
    if config.sqlite.is_some() {
        return Some(sqlite::SqliteSink::new(config));
    }

    #[cfg(feature = "mqtt")]
    if config.mqtt.is_some() {
        return Some(mqtt::MqttSink::new(config));
    }

    None
}