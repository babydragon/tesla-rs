use std::thread;
use std::time::Duration;
use rumqttc::{Client, MqttOptions, QoS};
use tesla::FullVehicleData;
use crate::Config;
use crate::sink::Sink;

pub struct MqttSink {
    client: Client,
    topic: String,
}

impl MqttSink {
    pub fn new(config: Config) -> Box<dyn Sink> {
        let mqtt_config = config.mqtt.unwrap();
        let mut options = MqttOptions::new("teslac", mqtt_config.host, mqtt_config.port);
        if let (Some(username), Some(password)) = (mqtt_config.username, mqtt_config.password) {
            options.set_credentials(username, password);
        }
        options.set_keep_alive(Duration::from_secs(10));

        let (client, mut connection) = Client::new(options, 10);
        thread::spawn(move || {
            loop {
                connection.iter().next();   // ignore result
            }
        });
        Box::new(MqttSink {
            client,
            topic: mqtt_config.topic,
        })
    }

    fn send(&mut self, vehicle_data: &FullVehicleData) {
        if let Ok(json_content) = serde_json::to_string(vehicle_data) {
            info!("Sending data to MQTT: {}", json_content);
            let publish_result = self.client.publish(&self.topic, QoS::AtLeastOnce, false, json_content.as_bytes());

            if let Err(e) = publish_result {
                error!("Error publishing to MQTT: {:?}", e);
            }
        }
    }
}

impl Sink for MqttSink {
    fn save(&mut self, vehicle_data: &FullVehicleData) {
        self.send(vehicle_data);
    }

    fn destroy(&mut self) {
        let _ = self.client.disconnect();
    }
}