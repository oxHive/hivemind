use mdns_sd::{ServiceDaemon, ServiceEvent, ServiceInfo};

const SERVICE_TYPE: &str = "_hivemind._tcp.local.";

pub fn service_name(device_id: &str) -> String {
    format!("{device_id}.{SERVICE_TYPE}")
}

pub struct HiveDiscovery {
    daemon: ServiceDaemon,
}

impl HiveDiscovery {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self { daemon: ServiceDaemon::new()? })
    }

    pub fn advertise(&self, device_id: &str, name: &str, port: u16) -> anyhow::Result<()> {
        let host_ip = local_ip_address::local_ip()?.to_string();
        let info = ServiceInfo::new(
            SERVICE_TYPE,
            device_id,
            &format!("{device_id}.local."),
            host_ip,
            port,
            &[("name", name)][..],
        )?;
        self.daemon.register(info)?;
        Ok(())
    }

    pub fn browse(&self) -> anyhow::Result<mdns_sd::Receiver<ServiceEvent>> {
        Ok(self.daemon.browse(SERVICE_TYPE)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_name_includes_device_id_and_service_type() {
        let name = service_name("hive_abc123");
        assert_eq!(name, "hive_abc123._hivemind._tcp.local.");
    }
}
