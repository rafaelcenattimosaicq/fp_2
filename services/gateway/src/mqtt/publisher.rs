use crate::config::MqttConfig;
use crate::state::{ConnectionStatus, SharedState};
use rumqttc::{AsyncClient, MqttOptions, QoS};
use std::time::Duration;
pub fn parse_broker_url(url: &str) -> (String, u16) {
    let s=url.strip_prefix("mqtt://").or_else(||url.strip_prefix("tcp://")).unwrap_or(url);
    if let Some((h,p))=s.rsplit_once(':'){(h.into(),p.parse().unwrap_or(1883))}else{(s.into(),1883)}
}
pub fn run_mqtt_loop(config: &MqttConfig, state: SharedState) -> AsyncClient {
    let (host,port)=parse_broker_url(&config.broker_url);
    let cid=state.read().map(|s|s.gateway_id.clone()).unwrap_or_default();
    let mut opts=MqttOptions::new(&cid,&host,port);
    opts.set_keep_alive(Duration::from_secs(30));
    if let Ok(mut s)=state.write(){s.mqtt_status=ConnectionStatus::Connecting;}
    let (client,mut evloop)=AsyncClient::new(opts,10);
    tokio::spawn(async move{loop{match evloop.poll().await{
        Ok(_)=>{if let Ok(mut s)=state.write(){s.mqtt_status=ConnectionStatus::Connected;}}
        Err(e)=>{if let Ok(mut s)=state.write(){s.mqtt_status=ConnectionStatus::Error(format!("{e}"));}tokio::time::sleep(Duration::from_secs(3)).await;}
    }}});
    client
}
pub const fn qos_from_u8(l: u8) -> QoS { match l { 0=>QoS::AtMostOnce, 2=>QoS::ExactlyOnce, _=>QoS::AtLeastOnce } }
