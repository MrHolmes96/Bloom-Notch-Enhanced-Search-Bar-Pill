//! Bluetooth device management for the notch's Bluetooth view: paired devices
//! with their type, connection and battery; connecting and disconnecting
//! headphones and speakers; pairing new devices, with PIN prompts answered in
//! Bloom's own UI; and removing devices.
//!
//! Battery: what Windows shows in Settings (a property of the device), else
//! what a Bluetooth LE device says through its Battery Service, and for Xbox
//! controllers what Windows.Gaming.Input reports, which also says whether it
//! is charging. A monitor keeps the connected devices' levels fresh for the
//! notch's battery bubble and warns once when one runs low (Settings > Notch
//! > Bluetooth Battery).
//!
//! Windows has no public "connect this device" call. Audio devices are the
//! exception: their Bluetooth audio driver accepts a one-shot reconnect or
//! disconnect request, the same one the Settings app sends. Other devices
//! (mice, keyboards, phones) connect themselves when they're in use, so they
//! show their status and can be removed, but aren't connected from here.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter};
use windows::core::{Interface, GUID, HSTRING};
use windows::Devices::Bluetooth::{BluetoothDevice, BluetoothLEDevice};
use windows::Devices::Enumeration::{
    DeviceInformation, DeviceInformationKind, DeviceInformationUpdate, DevicePairingKinds,
    DevicePairingRequestedEventArgs, DevicePairingResultStatus, DeviceUnpairingResultStatus,
    DeviceWatcher,
};
use windows::Foundation::{IReference, TypedEventHandler};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_ContainerId;
use windows::Win32::Media::Audio::{
    eAll, IDeviceTopology, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE,
    DEVICE_STATE_ACTIVE, DEVICE_STATE_UNPLUGGED,
};
use windows::Win32::Media::KernelStreaming::{
    IKsControl, KSPROPSETID_BtAudio, KSIDENTIFIER, KSIDENTIFIER_0, KSIDENTIFIER_0_0,
    KSPROPERTY_ONESHOT_DISCONNECT, KSPROPERTY_ONESHOT_RECONNECT, KSPROPERTY_TYPE_GET,
};
use windows::Win32::System::Com::StructuredStorage::PropVariantToGUID;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED, STGM_READ,
};
use windows_collections::IIterable;

const PROP_CONNECTED: &str = "System.Devices.Aep.IsConnected";
const PROP_CONTAINER: &str = "System.Devices.Aep.ContainerId";
/// The battery level Windows shows in Settings, on the device's PnP nodes.
const PROP_BATTERY: &str = "{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2";
/// How long Windows waits for the user to answer a pairing prompt.
const PAIRING_ANSWER_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Clone, Serialize)]
pub struct BtDevice {
    pub id: String,
    pub name: String,
    /// "earbuds", "headset", "headphones", "speaker", "audio", "phone",
    /// "computer", "keyboard", "mouse", "gamepad", "wearable" or "other".
    pub kind: &'static str,
    pub connected: bool,
    /// Percent, when the device reports it.
    pub battery: Option<u8>,
    /// Whether it is charging, when it says (Xbox controllers do).
    pub charging: Option<bool>,
    /// Bloom can connect and disconnect it (headphones and speakers).
    pub can_connect: bool,
}

#[derive(Clone, Serialize)]
pub struct BtNearbyDevice {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
}

#[derive(Clone, Serialize)]
pub struct BtPairingRequest {
    pub id: String,
    /// "confirm" (does this PIN match the device?), "display" (type this PIN
    /// on the device) or "provide" (enter the PIN shown by the device).
    pub kind: &'static str,
    pub pin: Option<String>,
}

fn com_init() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
    }
}

fn props(keys: &[&str]) -> IIterable<HSTRING> {
    keys.iter()
        .map(|k| HSTRING::from(*k))
        .collect::<Vec<_>>()
        .into()
}

fn prop<T: windows::core::RuntimeType + 'static>(info: &DeviceInformation, key: &str) -> Option<T> {
    info.Properties()
        .ok()?
        .Lookup(&HSTRING::from(key))
        .ok()?
        .cast::<IReference<T>>()
        .ok()?
        .Value()
        .ok()
}

fn find_all(aqs: &HSTRING, keys: &[&str], kind: DeviceInformationKind) -> Vec<DeviceInformation> {
    let Ok(found) = DeviceInformation::FindAllAsyncWithKindAqsFilterAndAdditionalProperties(
        aqs,
        &props(keys),
        kind,
    )
    .and_then(|op| op.get()) else {
        return Vec::new();
    };
    (0..found.Size().unwrap_or(0))
        .filter_map(|i| found.GetAt(i).ok())
        .collect()
}

/// Device type from a classic device's class of device: the major class, and
/// for peripherals the keyboard/pointer bits and joystick/gamepad minor class.
fn classic_kind(raw: u32) -> &'static str {
    let major = (raw >> 8) & 0x1F;
    let minor = (raw >> 2) & 0x3F;
    match major {
        1 => "computer",
        2 => "phone",
        4 => match minor {
            // Wearable headset, hands-free.
            0x01 | 0x02 => "headset",
            0x06 => "headphones",
            // Loudspeaker, portable audio, car audio, hi-fi.
            0x05 | 0x07 | 0x08 | 0x0A => "speaker",
            _ => "audio",
        },
        5 if minor & 0x10 != 0 => "keyboard",
        5 if minor & 0x20 != 0 => "mouse",
        5 if matches!(minor & 0x0F, 1 | 2) => "gamepad",
        7 => "wearable",
        _ => "other",
    }
}

/// Device type from a Bluetooth LE appearance category (and HID subcategory).
fn le_kind(category: u16, subcategory: u16) -> &'static str {
    match category {
        0x01 => "phone",
        0x02 => "computer",
        0x03 | 0x31 => "wearable",
        0x0F => match subcategory {
            0x01 => "keyboard",
            0x02 => "mouse",
            0x03 | 0x04 => "gamepad",
            _ => "other",
        },
        // Wearable audio device: earbud, headset, headphones, neck band.
        0x25 => match subcategory {
            0x01 => "earbuds",
            0x02 | 0x04 => "headset",
            0x03 => "headphones",
            _ => "audio",
        },
        0x21 => "speaker",
        0x22 | 0x29 => "audio",
        _ => "other",
    }
}

/// Earbuds usually call themselves headphones or a headset to Windows; their
/// names tell them apart.
fn refine_kind(kind: &'static str, name: &str) -> &'static str {
    let lower = name.to_lowercase();
    let earbuds = [
        "buds", "pods", "earbud", "earphone", "in-ear", "wf-", "elite", "liberty",
    ];
    if matches!(kind, "audio" | "headset" | "headphones")
        && earbuds.iter().any(|word| lower.contains(word))
    {
        return "earbuds";
    }
    if kind == "other" && (lower.contains("controller") || lower.contains("gamepad")) {
        return "gamepad";
    }
    kind
}

fn device_kind(id: &HSTRING) -> &'static str {
    if let Ok(device) = BluetoothDevice::FromIdAsync(id).and_then(|op| op.get()) {
        if let Ok(raw) = device.ClassOfDevice().and_then(|c| c.RawValue()) {
            return classic_kind(raw);
        }
    }
    if let Ok(device) = BluetoothLEDevice::FromIdAsync(id).and_then(|op| op.get()) {
        if let Ok(appearance) = device.Appearance() {
            return le_kind(
                appearance.Category().unwrap_or(0),
                appearance.SubCategory().unwrap_or(0),
            );
        }
    }
    "other"
}

fn battery_level(container: &GUID) -> Option<u8> {
    let aqs = HSTRING::from(format!("System.Devices.ContainerId:=\"{{{container:?}}}\""));
    find_all(&aqs, &[PROP_BATTERY], DeviceInformationKind::Device)
        .iter()
        .find_map(|info| prop::<u8>(info, PROP_BATTERY))
        .filter(|level| *level <= 100)
}

fn selectors(paired: bool) -> Vec<HSTRING> {
    [
        BluetoothDevice::GetDeviceSelectorFromPairingState(paired),
        BluetoothLEDevice::GetDeviceSelectorFromPairingState(paired),
    ]
    .into_iter()
    .filter_map(Result::ok)
    .collect()
}

/// Bluetooth audio driver filters for each device container, so audio devices
/// can be connected and disconnected.
fn audio_filters() -> HashMap<GUID, Vec<String>> {
    let mut filters: HashMap<GUID, Vec<String>> = HashMap::new();
    unsafe {
        let Ok(enumerator) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        else {
            return filters;
        };
        let Ok(endpoints) = enumerator.EnumAudioEndpoints(
            eAll,
            DEVICE_STATE(DEVICE_STATE_ACTIVE.0 | DEVICE_STATE_UNPLUGGED.0),
        ) else {
            return filters;
        };
        for i in 0..endpoints.GetCount().unwrap_or(0) {
            let Ok(endpoint) = endpoints.Item(i) else {
                continue;
            };
            let container = endpoint
                .OpenPropertyStore(STGM_READ)
                .and_then(|store| store.GetValue(&PKEY_Device_ContainerId))
                .and_then(|value| PropVariantToGUID(&value));
            let Ok(container) = container else {
                continue;
            };
            // The endpoint's first connector leads to the driver filter that
            // owns the Bluetooth audio link.
            let filter_id = endpoint
                .Activate::<IDeviceTopology>(CLSCTX_ALL, None)
                .and_then(|topology| topology.GetConnector(0))
                .and_then(|connector| connector.GetDeviceIdConnectedTo());
            let Ok(filter_id) = filter_id else {
                continue;
            };
            let id = filter_id.to_string().unwrap_or_default();
            CoTaskMemFree(Some(filter_id.0 as *const _));
            let entry = filters.entry(container).or_default();
            if !id.is_empty() && !entry.contains(&id) {
                entry.push(id);
            }
        }
    }
    filters
}

/// Sends a one-shot reconnect or disconnect to a Bluetooth audio filter.
/// Fails for filters that aren't Bluetooth audio.
fn send_oneshot(filter_id: &str, connect: bool) -> windows::core::Result<()> {
    unsafe {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let filter = enumerator.GetDevice(&HSTRING::from(filter_id))?;
        let control: IKsControl = filter.Activate(CLSCTX_ALL, None)?;
        let request = if connect {
            KSPROPERTY_ONESHOT_RECONNECT
        } else {
            KSPROPERTY_ONESHOT_DISCONNECT
        };
        let property = KSIDENTIFIER {
            Anonymous: KSIDENTIFIER_0 {
                Anonymous: KSIDENTIFIER_0_0 {
                    Set: KSPROPSETID_BtAudio,
                    Id: request.0 as u32,
                    Flags: KSPROPERTY_TYPE_GET,
                },
            },
        };
        let mut returned = 0u32;
        control.KsProperty(
            &property,
            std::mem::size_of::<KSIDENTIFIER>() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
        )
    }
}

/// Paired devices' containers, from the last listing, for connect requests.
static CONTAINERS: Mutex<Option<HashMap<String, GUID>>> = Mutex::new(None);

fn list_sync() -> Vec<BtDevice> {
    com_init();
    let audio = audio_filters();
    let mut by_container: HashMap<GUID, BtDevice> = HashMap::new();
    let mut unknown_container = Vec::new();
    let mut containers = HashMap::new();
    // Connected Bluetooth LE devices by name, for reading their Battery Service.
    let mut le_ids: HashMap<String, String> = HashMap::new();

    for selector in selectors(true) {
        for info in find_all(
            &selector,
            &[PROP_CONNECTED, PROP_CONTAINER],
            DeviceInformationKind::AssociationEndpoint,
        ) {
            let Ok(id) = info.Id() else { continue };
            let name = info.Name().map(|n| n.to_string()).unwrap_or_default();
            let connected = prop::<bool>(&info, PROP_CONNECTED).unwrap_or(false);
            let container = prop::<GUID>(&info, PROP_CONTAINER);
            let kind = refine_kind(device_kind(&id), &name);
            if connected && id.to_string().starts_with("BluetoothLE#") {
                le_ids.insert(name.to_lowercase(), id.to_string());
            }
            let device = BtDevice {
                id: id.to_string(),
                name,
                kind,
                connected,
                battery: None,
                charging: None,
                can_connect: container.is_some_and(|c| audio.contains_key(&c)),
            };
            match container {
                // A device paired over both classic and LE shows up twice;
                // keep one entry, connected if either link is.
                Some(container) => {
                    containers.insert(device.id.clone(), container);
                    by_container
                        .entry(container)
                        .and_modify(|existing| {
                            existing.connected |= device.connected;
                            if existing.kind == "other" {
                                existing.kind = device.kind;
                            }
                        })
                        .or_insert(device);
                }
                None => unknown_container.push(device),
            }
        }
    }

    let devices: Vec<BtDevice> = by_container
        .into_iter()
        .map(|(container, mut device)| {
            device.battery = if device.connected {
                battery_level(&container)
            } else {
                None
            };
            device
        })
        .chain(unknown_container)
        .filter(|d| !d.name.is_empty())
        .collect();
    // Windows can file a device's classic and LE links under different
    // containers (phones do this), so entries with the same name are one device.
    let mut merged: Vec<BtDevice> = Vec::with_capacity(devices.len());
    for device in devices {
        match merged.iter_mut().find(|d| d.name == device.name) {
            Some(existing) => {
                existing.connected |= device.connected;
                existing.can_connect |= device.can_connect;
                existing.battery = existing.battery.or(device.battery);
                // Keep the entry whose type is known, as it's the one Windows
                // describes fully.
                if existing.kind == "other" && device.kind != "other" {
                    existing.id = device.id;
                    existing.kind = device.kind;
                }
            }
            None => merged.push(device),
        }
    }
    let mut devices = merged;
    // What the monitor read itself, for devices Windows has no level for.
    let extra = EXTRA_BATTERY
        .lock()
        .ok()
        .and_then(|e| e.clone())
        .unwrap_or_default();
    for device in devices.iter_mut().filter(|d| d.connected) {
        if let Some(&(level, charging)) = extra.get(&device.name.to_lowercase()) {
            if device.battery.is_none() {
                device.battery = Some(level);
            }
            device.charging = charging.or(device.charging);
        }
    }
    if let Ok(mut known) = LE_IDS.lock() {
        *known = Some(le_ids);
    }
    devices.sort_by(|a, b| {
        b.connected
            .cmp(&a.connected)
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    if let Ok(mut cache) = CONTAINERS.lock() {
        *cache = Some(containers);
    }
    devices
}

#[tauri::command]
pub async fn bluetooth_list_devices() -> Result<Vec<BtDevice>, String> {
    tauri::async_runtime::spawn_blocking(list_sync)
        .await
        .map_err(|e| e.to_string())
}

/// Connects or disconnects a paired audio device.
#[tauri::command]
pub async fn bluetooth_set_connected(id: String, connected: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        com_init();
        let container = CONTAINERS
            .lock()
            .ok()
            .and_then(|cache| cache.as_ref()?.get(&id).copied())
            .ok_or("not-found")?;
        let filters = audio_filters().remove(&container).ok_or("unsupported")?;
        // A headset has a filter per profile (stereo audio, hands-free); a
        // request on any of them moves the whole device.
        let mut last_error = None;
        let mut any_ok = false;
        for filter in &filters {
            match send_oneshot(filter, connected) {
                Ok(()) => {
                    any_ok = true;
                    if connected {
                        break;
                    }
                }
                Err(e) => last_error = Some(e),
            }
        }
        if any_ok {
            Ok(())
        } else {
            Err(last_error
                .map(|e| e.message())
                .unwrap_or_else(|| "unsupported".into()))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Battery levels Windows doesn't show, and the monitor ──────────────────────

/// Levels read by the monitor (device name, lower case, to percent and
/// whether it is charging when known).
type Extra = HashMap<String, (u8, Option<bool>)>;
static EXTRA_BATTERY: Mutex<Option<Extra>> = Mutex::new(None);
/// Connected Bluetooth LE devices by name, from the last listing.
static LE_IDS: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);
/// Wakes the monitor (a device connected, disconnected or changed).
static MONITOR_WAKE: OnceLock<Sender<()>> = OnceLock::new();

/// A Bluetooth LE device's battery, from its Battery Service.
fn gatt_battery(le_id: &str) -> Option<u8> {
    use windows::Devices::Bluetooth::BluetoothCacheMode;
    use windows::Devices::Bluetooth::GenericAttributeProfile::{
        GattCharacteristicUuids, GattCommunicationStatus, GattServiceUuids,
    };
    use windows::Storage::Streams::DataReader;
    let device = BluetoothLEDevice::FromIdAsync(&HSTRING::from(le_id))
        .and_then(|op| op.get())
        .ok()?;
    let level = (|| -> Option<u8> {
        let services = device
            .GetGattServicesForUuidWithCacheModeAsync(
                GattServiceUuids::Battery().ok()?,
                BluetoothCacheMode::Cached,
            )
            .and_then(|op| op.get())
            .ok()?;
        if services.Status().ok()? != GattCommunicationStatus::Success {
            return None;
        }
        let list = services.Services().ok()?;
        if list.Size().ok()? == 0 {
            return None;
        }
        let service = list.GetAt(0).ok()?;
        let read = (|| -> Option<u8> {
            let characteristics = service
                .GetCharacteristicsForUuidWithCacheModeAsync(
                    GattCharacteristicUuids::BatteryLevel().ok()?,
                    BluetoothCacheMode::Cached,
                )
                .and_then(|op| op.get())
                .ok()?;
            let found = characteristics.Characteristics().ok()?;
            if found.Size().ok()? == 0 {
                return None;
            }
            let value = found
                .GetAt(0)
                .ok()?
                .ReadValueWithCacheModeAsync(BluetoothCacheMode::Uncached)
                .and_then(|op| op.get())
                .ok()?;
            if value.Status().ok()? != GattCommunicationStatus::Success {
                return None;
            }
            let reader = DataReader::FromBuffer(&value.Value().ok()?).ok()?;
            reader.ReadByte().ok()
        })();
        let _ = service.Close();
        read
    })();
    let _ = device.Close();
    level.filter(|level| *level <= 100)
}

/// Xbox (and other Windows.Gaming.Input) controllers' batteries, by name.
fn gamepad_batteries() -> Extra {
    use windows::Gaming::Input::{Gamepad, RawGameController};
    use windows::System::Power::BatteryStatus;
    let mut found = Extra::new();
    let Ok(pads) = Gamepad::Gamepads() else {
        return found;
    };
    for index in 0..pads.Size().unwrap_or(0) {
        let Ok(pad) = pads.GetAt(index) else { continue };
        let Some(name) = RawGameController::FromGameController(&pad)
            .and_then(|raw| raw.DisplayName())
            .ok()
            .map(|n| n.to_string().to_lowercase())
        else {
            continue;
        };
        let Ok(report) = pad.TryGetBatteryReport() else {
            continue;
        };
        let full = report
            .FullChargeCapacityInMilliwattHours()
            .and_then(|v| v.Value())
            .ok();
        let left = report
            .RemainingCapacityInMilliwattHours()
            .and_then(|v| v.Value())
            .ok();
        let (Some(full), Some(left)) = (full, left) else {
            continue;
        };
        if full <= 0 {
            continue;
        }
        let level = ((left as f64 / full as f64) * 100.0)
            .round()
            .clamp(0.0, 100.0) as u8;
        let charging = report.Status().ok().and_then(|status| match status {
            BatteryStatus::Charging => Some(true),
            BatteryStatus::Discharging | BatteryStatus::Idle => Some(false),
            _ => None,
        });
        found.insert(name, (level, charging));
    }
    found
}

fn setting_number(app: &AppHandle, key: &str, fallback: u32) -> u32 {
    crate::utils::get_setting_str(app, key)
        .and_then(|v| v.trim().parse::<u32>().ok())
        .unwrap_or(fallback)
}

/// Keeps the connected devices' battery levels fresh (every minute, and at
/// once when a device connects or changes) for the notch's battery bubble
/// ("bluetooth-devices"), and warns once when a device drops to the level
/// set in Settings > Notch > Bluetooth Battery (again only after it charged).
pub fn start_battery_monitor(app: AppHandle) {
    let (wake, woken) = channel::<()>();
    if MONITOR_WAKE.set(wake).is_err() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("bloom-bt-battery".into())
        .spawn(move || {
            com_init();
            // Devices already warned about, until they charge back above.
            let mut warned: std::collections::HashSet<String> = Default::default();
            let mut gatt_read_at: HashMap<String, std::time::Instant> = HashMap::new();
            // Every minute only while a device is connected; otherwise it
            // waits for Windows to say one connected.
            let mut any_connected = true;
            loop {
                if any_connected {
                    let _ = woken.recv_timeout(Duration::from_secs(60));
                } else if woken.recv().is_err() {
                    return;
                }
                // A burst of changes (a device connecting) is one update.
                std::thread::sleep(Duration::from_millis(400));
                while woken.try_recv().is_ok() {}
                if crate::state::SHUTTING_DOWN.load(std::sync::atomic::Ordering::Relaxed) {
                    continue;
                }
                // Nothing to do with the bubble and the warning both off.
                let bubble = crate::utils::get_setting_str(&app, "bloom-bt-bubble").as_deref()
                    != Some("false");
                if !bubble && setting_number(&app, "bloom-bt-low-battery", 20) == 0 {
                    continue;
                }
                let mut devices = list_sync();
                any_connected = devices.iter().any(|d| d.connected);
                // Levels Windows doesn't show: controllers every time (cheap),
                // a Battery Service every five minutes per device (it may wake
                // the device).
                let mut extra = gamepad_batteries();
                let le_ids = LE_IDS
                    .lock()
                    .ok()
                    .and_then(|m| m.clone())
                    .unwrap_or_default();
                let old = EXTRA_BATTERY
                    .lock()
                    .ok()
                    .and_then(|e| e.clone())
                    .unwrap_or_default();
                for device in devices
                    .iter()
                    .filter(|d| d.connected && d.battery.is_none())
                {
                    let key = device.name.to_lowercase();
                    if extra.contains_key(&key) {
                        continue;
                    }
                    let due = gatt_read_at
                        .get(&key)
                        .is_none_or(|at| at.elapsed() >= Duration::from_secs(300));
                    if due {
                        if let Some(id) = le_ids.get(&key) {
                            gatt_read_at.insert(key.clone(), std::time::Instant::now());
                            if let Some(level) = gatt_battery(id) {
                                extra.insert(key.clone(), (level, None));
                            }
                        }
                    } else if let Some(previous) = old.get(&key) {
                        extra.insert(key.clone(), *previous);
                    }
                }
                if let Ok(mut saved) = EXTRA_BATTERY.lock() {
                    *saved = Some(extra.clone());
                }
                for device in devices.iter_mut().filter(|d| d.connected) {
                    if let Some(&(level, charging)) = extra.get(&device.name.to_lowercase()) {
                        device.battery = device.battery.or(Some(level));
                        device.charging = charging.or(device.charging);
                    }
                }
                emit("bluetooth-devices", devices.clone());

                let threshold = setting_number(&app, "bloom-bt-low-battery", 20).min(95);
                for device in &devices {
                    let key = device.name.to_lowercase();
                    let Some(level) = device.battery.filter(|_| device.connected) else {
                        continue;
                    };
                    if threshold == 0
                        || device.charging == Some(true)
                        || level as u32 > threshold + 5
                    {
                        warned.remove(&key);
                        continue;
                    }
                    if level as u32 <= threshold && warned.insert(key) {
                        crate::notifications::show_bloom_notice(
                            &app,
                            &format!("{} battery low", device.name),
                            &format!("{level}% left. Charge it soon."),
                        );
                    }
                }
            }
        });
}

fn wake_monitor() {
    if let Some(wake) = MONITOR_WAKE.get() {
        let _ = wake.send(());
    }
}

/// The connected devices with their batteries, at once (the notch asks when
/// it starts, then hears "bluetooth-devices").
#[tauri::command]
pub fn bluetooth_battery_refresh() {
    wake_monitor();
}

// ── Watching ─────────────────────────────────────────────────────────────────

static APP: OnceLock<AppHandle> = OnceLock::new();
/// Keeps the paired-device watchers alive for the app's lifetime.
static PAIRED_WATCHERS: Mutex<Vec<DeviceWatcher>> = Mutex::new(Vec::new());

fn emit<S: Serialize + Clone>(event: &str, payload: S) {
    if let Some(app) = APP.get() {
        let _ = app.emit(event, payload);
    }
}

fn watch(
    selector: &HSTRING,
    on_added: impl Fn(&DeviceInformation) + Send + Sync + 'static,
    on_updated: impl Fn(&DeviceInformationUpdate) + Send + Sync + 'static,
    on_removed: impl Fn(&DeviceInformationUpdate) + Send + Sync + 'static,
) -> windows::core::Result<DeviceWatcher> {
    let watcher = DeviceInformation::CreateWatcherWithKindAqsFilterAndAdditionalProperties(
        selector,
        &props(&[PROP_CONNECTED, PROP_CONTAINER]),
        DeviceInformationKind::AssociationEndpoint,
    )?;
    watcher.Added(&TypedEventHandler::new(
        move |_, info: windows::core::Ref<DeviceInformation>| {
            if let Some(info) = info.as_ref() {
                on_added(info);
            }
            Ok(())
        },
    ))?;
    watcher.Updated(&TypedEventHandler::new(
        move |_, update: windows::core::Ref<DeviceInformationUpdate>| {
            if let Some(update) = update.as_ref() {
                on_updated(update);
            }
            Ok(())
        },
    ))?;
    watcher.Removed(&TypedEventHandler::new(
        move |_, update: windows::core::Ref<DeviceInformationUpdate>| {
            if let Some(update) = update.as_ref() {
                on_removed(update);
            }
            Ok(())
        },
    ))?;
    watcher.Start()?;
    Ok(watcher)
}

/// Watches paired devices so the view and the controls tile update when a
/// device connects, disconnects, pairs or is removed.
pub fn setup_bluetooth_notifications(app: AppHandle) {
    let _ = APP.set(app);
    std::thread::spawn(|| {
        com_init();
        let mut watchers = Vec::new();
        for selector in selectors(true) {
            let changed = || {
                emit("bluetooth-changed", ());
                wake_monitor();
            };
            if let Ok(watcher) = watch(
                &selector,
                move |_| changed(),
                move |_| changed(),
                move |_| changed(),
            ) {
                watchers.push(watcher);
            }
        }
        if let Ok(mut kept) = PAIRED_WATCHERS.lock() {
            *kept = watchers;
        }
    });
}

static NEARBY: Mutex<Option<HashMap<String, BtNearbyDevice>>> = Mutex::new(None);
static NEARBY_WATCHERS: Mutex<Vec<DeviceWatcher>> = Mutex::new(Vec::new());

fn emit_nearby() {
    let list = NEARBY
        .lock()
        .ok()
        .and_then(|nearby| {
            let mut list: Vec<_> = nearby.as_ref()?.values().cloned().collect();
            list.sort_by_key(|d| d.name.to_lowercase());
            Some(list)
        })
        .unwrap_or_default();
    emit("bluetooth-nearby", list);
}

/// Starts or stops looking for devices that can be paired. Results arrive as
/// `bluetooth-nearby` events with the full list.
#[tauri::command]
pub async fn bluetooth_discovery(active: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        com_init();
        if let Ok(mut watchers) = NEARBY_WATCHERS.lock() {
            for watcher in watchers.drain(..) {
                let _ = watcher.Stop();
            }
        }
        if let Ok(mut nearby) = NEARBY.lock() {
            *nearby = active.then(HashMap::new);
        }
        if !active {
            return Ok(());
        }
        emit_nearby();
        let mut started = Vec::new();
        for selector in selectors(false) {
            let watcher = watch(
                &selector,
                |info| {
                    let (Ok(id), Ok(name)) = (info.Id(), info.Name()) else {
                        return;
                    };
                    // Unnamed advertisements (beacons, trackers) aren't pairable
                    // in any useful way; Windows' own list hides them too.
                    if name.is_empty() {
                        return;
                    }
                    let device = BtNearbyDevice {
                        id: id.to_string(),
                        name: name.to_string(),
                        kind: device_kind(&id),
                    };
                    if let Ok(mut nearby) = NEARBY.lock() {
                        if let Some(nearby) = nearby.as_mut() {
                            nearby.insert(device.id.clone(), device);
                        }
                    }
                    emit_nearby();
                },
                |_| {},
                |update| {
                    let Ok(id) = update.Id() else { return };
                    if let Ok(mut nearby) = NEARBY.lock() {
                        if let Some(nearby) = nearby.as_mut() {
                            nearby.remove(&id.to_string());
                        }
                    }
                    emit_nearby();
                },
            )
            .map_err(|e| e.message())?;
            started.push(watcher);
        }
        if let Ok(mut watchers) = NEARBY_WATCHERS.lock() {
            *watchers = started;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Pairing ──────────────────────────────────────────────────────────────────

/// Where the view's answer to the current pairing prompt goes: None rejects,
/// Some(pin) accepts (the PIN is only used when the device asked for one).
static PAIRING_ANSWER: Mutex<Option<Sender<Option<String>>>> = Mutex::new(None);

fn answer_request(id: &str, args: &DevicePairingRequestedEventArgs) -> windows::core::Result<()> {
    let kinds = args.PairingKind()?;
    let pin = args
        .Pin()
        .ok()
        .map(|p| p.to_string())
        .filter(|p| !p.is_empty());
    let kind = if kinds == DevicePairingKinds::ConfirmOnly {
        // The user already chose to pair by clicking Pair.
        return args.Accept();
    } else if kinds == DevicePairingKinds::DisplayPin {
        "display"
    } else if kinds == DevicePairingKinds::ConfirmPinMatch {
        "confirm"
    } else if kinds == DevicePairingKinds::ProvidePin {
        "provide"
    } else {
        return Ok(());
    };

    // The handler may block: Windows waits for it (or its deferral) before
    // continuing, and this runs on a thread-pool thread.
    let (tx, rx) = channel();
    if let Ok(mut answer) = PAIRING_ANSWER.lock() {
        *answer = Some(tx);
    }
    emit(
        "bluetooth-pairing-request",
        BtPairingRequest {
            id: id.to_string(),
            kind,
            pin,
        },
    );
    if kind == "display" {
        // Nothing to answer: the user types the PIN on the device itself.
        return args.Accept();
    }
    match rx.recv_timeout(PAIRING_ANSWER_TIMEOUT) {
        Ok(Some(pin)) if kind == "provide" => args.AcceptWithPin(&HSTRING::from(pin)),
        Ok(Some(_)) => args.Accept(),
        _ => Ok(()),
    }
}

fn pairing_error(status: DevicePairingResultStatus) -> String {
    match status {
        DevicePairingResultStatus::AuthenticationTimeout => "timeout".into(),
        DevicePairingResultStatus::AuthenticationFailure
        | DevicePairingResultStatus::RejectedByHandler
        | DevicePairingResultStatus::PairingCanceled
        | DevicePairingResultStatus::OperationAlreadyInProgress => "cancelled".into(),
        DevicePairingResultStatus::ConnectionRejected
        | DevicePairingResultStatus::NoSupportedProfiles
        | DevicePairingResultStatus::Failed => "failed".into(),
        DevicePairingResultStatus::NotReadyToPair => "not-ready".into(),
        other => format!("Couldn't pair ({})", other.0),
    }
}

/// Pairs with a nearby device. PIN prompts arrive as
/// `bluetooth-pairing-request` events, answered with bluetooth_pairing_respond.
#[tauri::command]
pub async fn bluetooth_pair(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        com_init();
        let info = DeviceInformation::CreateFromIdAsync(&HSTRING::from(id.as_str()))
            .and_then(|op| op.get())
            .map_err(|e| e.message())?;
        let custom = info
            .Pairing()
            .and_then(|p| p.Custom())
            .map_err(|e| e.message())?;
        let handler_id = id.clone();
        let token = custom
            .PairingRequested(&TypedEventHandler::new(
                move |_, args: windows::core::Ref<DevicePairingRequestedEventArgs>| {
                    if let Some(args) = args.as_ref() {
                        answer_request(&handler_id, args)?;
                    }
                    Ok(())
                },
            ))
            .map_err(|e| e.message())?;
        let kinds = DevicePairingKinds(
            DevicePairingKinds::ConfirmOnly.0
                | DevicePairingKinds::DisplayPin.0
                | DevicePairingKinds::ProvidePin.0
                | DevicePairingKinds::ConfirmPinMatch.0,
        );
        let result = custom.PairAsync(kinds).and_then(|op| op.get());
        let _ = custom.RemovePairingRequested(token);
        if let Ok(mut answer) = PAIRING_ANSWER.lock() {
            *answer = None;
        }
        let status = result.and_then(|r| r.Status()).map_err(|e| e.message())?;
        if status == DevicePairingResultStatus::Paired
            || status == DevicePairingResultStatus::AlreadyPaired
        {
            emit("bluetooth-changed", ());
            Ok(())
        } else {
            Err(pairing_error(status))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Answers the pending pairing prompt.
#[tauri::command]
pub fn bluetooth_pairing_respond(accept: bool, pin: Option<String>) {
    if let Ok(mut answer) = PAIRING_ANSWER.lock() {
        if let Some(tx) = answer.take() {
            let _ = tx.send(accept.then(|| pin.unwrap_or_default()));
        }
    }
}

/// Removes a paired device.
#[tauri::command]
pub async fn bluetooth_unpair(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        com_init();
        let status = DeviceInformation::CreateFromIdAsync(&HSTRING::from(id.as_str()))
            .and_then(|op| op.get())
            .and_then(|info| info.Pairing())
            .and_then(|pairing| pairing.UnpairAsync())
            .and_then(|op| op.get())
            .and_then(|result| result.Status())
            .map_err(|e| e.message())?;
        if status == DeviceUnpairingResultStatus::Unpaired
            || status == DeviceUnpairingResultStatus::AlreadyUnpaired
        {
            emit("bluetooth-changed", ());
            Ok(())
        } else {
            Err(format!("Couldn't remove the device ({})", status.0))
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_classic_classes() {
        // Major class in bits 8-12, minor in bits 2-7.
        assert_eq!(classic_kind(0x24_0404), "audio");
        assert_eq!(classic_kind(0x5A_020C), "phone");
        assert_eq!(classic_kind(0x00_0540), "keyboard");
        assert_eq!(classic_kind(0x00_0580), "mouse");
        assert_eq!(classic_kind(0x00_0508), "gamepad");
        assert_eq!(classic_kind(0x00_010C), "computer");
        assert_eq!(classic_kind(0x00_0000), "other");
    }

    #[test]
    fn maps_le_appearance() {
        assert_eq!(le_kind(0x0F, 0x01), "keyboard");
        assert_eq!(le_kind(0x0F, 0x02), "mouse");
        assert_eq!(le_kind(0x03, 0), "wearable");
        assert_eq!(le_kind(0x25, 0), "audio");
        assert_eq!(le_kind(0x00, 0), "other");
    }
}
