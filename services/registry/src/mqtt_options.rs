use rumqttc::MqttOptions;

pub fn create_mqtt_options(s: &str, h: &str, p: u16) -> MqttOptions {
    let mut tmp = MqttOptions::new(s, h, p);
    tmp.set_keep_alive(std::time::Duration::from_secs(30));
    // clean_session=true so we dont get flooded with old messages on reconnect
    tmp.set_clean_session(true);
    tmp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_options_with_correct_client_id() {
        let client_id = "test-client";
        let host = "broker.example.com";
        let port = 1883;

        let opts = create_mqtt_options(client_id, host, port);

        assert_eq!(opts.client_id(), client_id);
    }

    #[test]
    fn sets_keep_alive_to_thirty_seconds() {
        let opts = create_mqtt_options("id", "localhost", 1883);

        assert_eq!(opts.keep_alive(), std::time::Duration::from_secs(30));
    }

    #[test]
    fn uses_specified_port() {
        let port = 8883;

        let opts = create_mqtt_options("id", "mqtt.local", port);

        let (_, broker_port) = opts.broker_address();
        assert_eq!(broker_port, port);
    }

    #[test]
    fn uses_specified_host() {
        let host = "custom-broker.internal";

        let opts = create_mqtt_options("id", host, 1883);

        let (broker_host, _) = opts.broker_address();
        assert_eq!(broker_host, host);
    }

    #[test]
    fn different_client_ids_produce_distinct_options() {
        let opts_a = create_mqtt_options("consumer-a", "localhost", 1883);
        let opts_b = create_mqtt_options("consumer-b", "localhost", 1883);

        assert_ne!(opts_a.client_id(), opts_b.client_id());
    }
}
