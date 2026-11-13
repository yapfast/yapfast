//! A microphone, a speaker or a camera, as the settings and a call name one.

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
}

/// Marks a device id made from the device's name, for a sound server whose
/// devices carry no GUID of their own and are picked by name instead.
pub const NAMED: &str = "name:";

/// WebRTC's PulseAudio devices have no GUID, and LiveKit selects only by GUID,
/// which would always pick the first device. Those devices go by name instead,
/// as `name:` and the name WebRTC reports.
pub fn device_id(guid: &str, name: &str) -> String {
    if guid.is_empty() {
        format!("{NAMED}{name}")
    } else {
        guid.into()
    }
}
