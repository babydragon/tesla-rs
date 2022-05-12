use std::time::{SystemTime, UNIX_EPOCH};
use rusqlite::Connection;
use rusqlite::params;
use tesla::{FullVehicleData};
use crate::config::Config;
use crate::sink::Sink;

pub struct SqliteSink {
    conn: Connection
}

const INSERT_BATTERY: &str = "INSERT INTO battery(ts, level, range) VALUES(?,?,?)";
const INSERT_DRIVER_STATE: &str = "INSERT INTO driver_state(ts, heading, latitude, longitude, power, speed) VALUES(?,?,?,?,?,?)";

impl SqliteSink {
    pub fn new(config: Config) -> Box<dyn Sink> {
        let file = config.sqlite.unwrap().file;
        let connection = Connection::open(file).expect("fail to open sqlite");

        // init tables
        let _ = connection.execute_batch("
            BEGIN;
            CREATE TABLE IF NOT EXISTS battery(ts INTEGER PRIMARY KEY, level INTEGER, range REAL) WITHOUT ROWID;
            CREATE TABLE IF NOT EXISTS driver_state(ts INTEGER PRIMARY KEY, heading INTEGER, latitude REAL, longitude REAL, power REAL, speed INTEGER) WITHOUT ROWID;
            COMMIT;
        ");

        Box::new(SqliteSink{
            conn: connection
        })
    }
}

impl Sink for SqliteSink {
    fn save(&self, vehicle_data: &FullVehicleData) {
        let time = SystemTime::now();
        let ts = time.duration_since(UNIX_EPOCH).unwrap().as_secs();

        if let Ok(mut stmt) = self.conn.prepare_cached(INSERT_BATTERY) {
            let _ = stmt.execute(params![ts, vehicle_data.charge_state.battery_level, vehicle_data.charge_state.battery_range * 1.6]);
        }

        if let Ok(mut stmt) = self.conn.prepare_cached(INSERT_DRIVER_STATE) {
            let _ = stmt.execute(params![ts, vehicle_data.drive_state.heading, vehicle_data.drive_state.latitude,
                vehicle_data.drive_state.longitude, vehicle_data.drive_state.power, vehicle_data.drive_state.speed]);
        }
    }

    fn destroy(&self) {
    }
}