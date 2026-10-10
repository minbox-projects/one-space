//! Platform bridge for the `ble-lock` tool (Step 1).
//!
//! On macOS this is a real CoreBluetooth bridge: it owns exactly one selected
//! [`CBPeripheral`] UUID, connects and periodically reads RSSI in active mode,
//! consumes advertisements (with `allowDuplicates`) in passive mode without ever
//! connecting, keeps name/id/latest-RSSI snapshots, cancels/disconnects on
//! selection replacement or clear, reports authorization/power state, and
//! suppresses stale callbacks for a replaced device.
//!
//! Everything runs on the main dispatch queue (`initWithDelegate:queue:` with a
//! `None` queue) and the delegate is retained by [`BleCentral`], so the whole
//! bridge is main-thread confined and never manufactures a `Send` handle from a
//! `!Send` CoreBluetooth object. The bridge only emits [`BleCentralEvent`]s to
//! its handler; the process-owned runtime added in Step 2 maps those onto
//! [`crate::ble_lock::state::BleLockMonitor`] events and drives the periodic
//! [`BleCentral::request_rssi`] cadence.
//!
//! On non-macOS targets a clear unavailable implementation exposes the same
//! surface and returns `platform_unavailable` for every native action.

use serde::{Deserialize, Serialize};

use crate::ble_lock::config::BleLockError;
use crate::ble_lock::state::{BleBluetoothState, BleDevice};

/// Bluetooth authorization status.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BleBluetoothAuthorization {
    /// The user has not yet been asked to grant Bluetooth access.
    #[default]
    NotDetermined,
    /// Access is restricted by policy.
    Restricted,
    /// Access was denied by the user.
    Denied,
    /// Access is granted for the lifetime of the app.
    AllowedAlways,
}

/// Events emitted by the CoreBluetooth bridge on the main queue.
///
/// The process-owned Step 2 runtime translates these into monitor
/// [`crate::ble_lock::state::BleLockEvent`]s plus status/`last_error` updates.
#[derive(Clone, Debug, PartialEq)]
pub enum BleCentralEvent {
    /// The central manager's state changed.
    State(BleBluetoothState),
    /// The authorization status changed.
    Authorization(BleBluetoothAuthorization),
    /// A peripheral was discovered or its snapshot was updated.
    Discovered(BleDevice),
    /// A valid advertisement RSSI; maps to a passive-mode monitor reading.
    Advertisement {
        /// CoreBluetooth UUID of the advertising device.
        device_id: String,
        /// Raw RSSI in dBm.
        rssi: i16,
    },
    /// A valid connected-RSSI reading; maps to an active-mode monitor reading.
    ConnectionRead {
        /// CoreBluetooth UUID of the connected device.
        device_id: String,
        /// Raw RSSI in dBm.
        rssi: i16,
    },
    /// The selected peripheral connected.
    Connected {
        /// CoreBluetooth UUID of the connected device.
        device_id: String,
    },
    /// The selected peripheral disconnected or failed to connect.
    Disconnected {
        /// CoreBluetooth UUID of the disconnected device.
        device_id: String,
    },
    /// A safe bridge-level error for the runtime to surface.
    Error(BleLockError),
}

/// Return `Some(rssi)` for a real measurement and `None` for the CoreBluetooth
/// "not available" sentinel (`127`). Values outside the configured threshold
/// range are still valid measurements.
pub(crate) fn normalize_rssi(raw: i16) -> Option<i16> {
    if raw == crate::ble_lock::state::INVALID_RSSI_SENTINEL {
        None
    } else {
        Some(raw)
    }
}

#[cfg(target_os = "macos")]
mod imp {
    use super::{BleBluetoothAuthorization, BleCentralEvent};
    use crate::ble_lock::config::{BleLockError, BleLockMode};
    use crate::ble_lock::state::BleBluetoothState;
    use crate::ble_lock::state::BleDevice;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyObject, ProtocolObject};
    use objc2::{define_class, msg_send, AnyThread, DefinedClass};
    use objc2_core_bluetooth::{
        CBAdvertisementDataLocalNameKey, CBCentralManager, CBCentralManagerDelegate,
        CBCentralManagerScanOptionAllowDuplicatesKey, CBManager, CBManagerAuthorization,
        CBManagerState, CBPeripheral, CBPeripheralDelegate,
    };
    use objc2_foundation::{
        NSArray, NSDictionary, NSError, NSNumber, NSObject, NSObjectProtocol, NSString, NSUUID,
    };
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::ptr::NonNull;

    type EventHandler = Box<dyn Fn(BleCentralEvent) + 'static>;

    struct DelegateState {
        handler: EventHandler,
        mode: Cell<BleLockMode>,
        scanning: Cell<bool>,
        selected_id: RefCell<Option<String>>,
        selected_name: RefCell<Option<String>>,
        peripheral: RefCell<Option<Retained<CBPeripheral>>>,
        peripherals: RefCell<HashMap<String, Retained<CBPeripheral>>>,
        devices: RefCell<HashMap<String, BleDevice>>,
    }

    define_class!(
        // SAFETY: `NSObject` has no subclassing requirements and this class does
        // not implement `Drop`.
        #[unsafe(super(NSObject))]
        #[ivars = DelegateState]
        struct CentralDelegate;

        unsafe impl NSObjectProtocol for CentralDelegate {}

        unsafe impl CBCentralManagerDelegate for CentralDelegate {
            #[unsafe(method(centralManagerDidUpdateState:))]
            unsafe fn central_manager_did_update_state(&self, central: &CBCentralManager) {
                self.handle_state(central);
            }

            #[unsafe(method(centralManager:didDiscoverPeripheral:advertisementData:RSSI:))]
            unsafe fn central_manager_did_discover(
                &self,
                _central: &CBCentralManager,
                peripheral: &CBPeripheral,
                advertisement_data: &NSDictionary<NSString, AnyObject>,
                rssi: &NSNumber,
            ) {
                self.handle_discover(peripheral, advertisement_data, rssi);
            }

            #[unsafe(method(centralManager:didConnectPeripheral:))]
            unsafe fn central_manager_did_connect(
                &self,
                _central: &CBCentralManager,
                peripheral: &CBPeripheral,
            ) {
                self.handle_connected(peripheral);
            }

            #[unsafe(method(centralManager:didFailToConnectPeripheral:error:))]
            unsafe fn central_manager_did_fail_to_connect(
                &self,
                _central: &CBCentralManager,
                peripheral: &CBPeripheral,
                error: Option<&NSError>,
            ) {
                self.handle_disconnected(peripheral, error);
            }

            #[unsafe(method(centralManager:didDisconnectPeripheral:error:))]
            unsafe fn central_manager_did_disconnect(
                &self,
                _central: &CBCentralManager,
                peripheral: &CBPeripheral,
                error: Option<&NSError>,
            ) {
                self.handle_disconnected(peripheral, error);
            }
        }

        unsafe impl CBPeripheralDelegate for CentralDelegate {
            #[unsafe(method(peripheral:didReadRSSI:error:))]
            unsafe fn peripheral_did_read_rssi(
                &self,
                peripheral: &CBPeripheral,
                rssi: &NSNumber,
                error: Option<&NSError>,
            ) {
                self.handle_read_rssi(peripheral, rssi, error);
            }
        }
    );

    impl CentralDelegate {
        fn emit(&self, event: BleCentralEvent) {
            (self.ivars().handler)(event);
        }

        fn handle_state(&self, central: &CBCentralManager) {
            let state = state_from(unsafe { central.state() });
            if state != BleBluetoothState::PoweredOn {
                // Below powered-on the system invalidates peripheral handles.
                self.ivars().scanning.set(false);
                self.ivars().peripheral.borrow_mut().take();
            }
            self.emit(BleCentralEvent::State(state));

            if state == BleBluetoothState::PoweredOn
                && self.ivars().mode.get() == BleLockMode::Active
            {
                let selected = self.ivars().selected_id.borrow().clone();
                if let Some(id) = selected {
                    if let Err(error) = self.connect_selected(central, &id) {
                        self.emit(BleCentralEvent::Error(error));
                    }
                }
            }
        }

        fn handle_discover(
            &self,
            peripheral: &CBPeripheral,
            advertisement_data: &NSDictionary<NSString, AnyObject>,
            rssi: &NSNumber,
        ) {
            let id = peripheral_id(peripheral);
            if id.is_empty() {
                return;
            }

            if let Some(retained) = unsafe { Retained::retain(NonNull::from(peripheral).as_ptr()) }
            {
                self.ivars()
                    .peripherals
                    .borrow_mut()
                    .insert(id.clone(), retained);
            }

            let reading = super::normalize_rssi(rssi.integerValue() as i16);
            let advertised_name =
                unsafe { advertisement_data.objectForKey(CBAdvertisementDataLocalNameKey) }
                    .as_deref()
                    .and_then(string_from);
            let name = advertised_name
                .or_else(|| unsafe { peripheral.name() }.map(|value| value.to_string()))
                .unwrap_or_else(|| id.clone());

            let device = {
                let mut devices = self.ivars().devices.borrow_mut();
                let entry = devices.entry(id.clone()).or_insert_with(|| BleDevice {
                    id: id.clone(),
                    name: name.clone(),
                    rssi: reading.unwrap_or(0),
                });
                entry.name = name;
                if let Some(value) = reading {
                    entry.rssi = value;
                }
                entry.clone()
            };
            self.emit(BleCentralEvent::Discovered(device));

            let selected = self.ivars().selected_id.borrow().clone();
            if selected.as_deref() == Some(id.as_str()) {
                if let Some(value) = reading {
                    if self.ivars().mode.get() == BleLockMode::Passive {
                        self.emit(BleCentralEvent::Advertisement {
                            device_id: id,
                            rssi: value,
                        });
                    }
                }
            }
        }

        fn handle_connected(&self, peripheral: &CBPeripheral) {
            let id = peripheral_id(peripheral);
            if id.is_empty() {
                return;
            }
            let selected = self.ivars().selected_id.borrow().clone();
            if selected.as_deref() != Some(id.as_str()) {
                // A connection we no longer own; never adopt it.
                return;
            }
            unsafe {
                peripheral.setDelegate(Some(ProtocolObject::from_ref(self)));
                peripheral.readRSSI();
            }
            *self.ivars().peripheral.borrow_mut() =
                unsafe { Retained::retain(NonNull::from(peripheral).as_ptr()) };
            self.emit(BleCentralEvent::Connected { device_id: id });
        }

        fn handle_disconnected(&self, peripheral: &CBPeripheral, _error: Option<&NSError>) {
            let id = peripheral_id(peripheral);
            let selected = self.ivars().selected_id.borrow().clone();
            if selected.as_deref() != Some(id.as_str()) {
                return;
            }
            self.ivars().peripheral.borrow_mut().take();
            self.emit(BleCentralEvent::Disconnected { device_id: id });
        }

        fn handle_read_rssi(
            &self,
            peripheral: &CBPeripheral,
            rssi: &NSNumber,
            error: Option<&NSError>,
        ) {
            if error.is_some() || self.ivars().mode.get() != BleLockMode::Active {
                return;
            }
            let id = peripheral_id(peripheral);
            let selected = self.ivars().selected_id.borrow().clone();
            if selected.as_deref() != Some(id.as_str()) {
                return;
            }
            if let Some(value) = super::normalize_rssi(rssi.integerValue() as i16) {
                self.emit(BleCentralEvent::ConnectionRead {
                    device_id: id,
                    rssi: value,
                });
            }
        }

        fn begin_scan(&self, central: &CBCentralManager) {
            unsafe {
                let key = CBCentralManagerScanOptionAllowDuplicatesKey;
                let value = NSNumber::numberWithBool(true);
                let value_ref: &AnyObject = &value;
                let options = NSDictionary::from_slices(&[key], &[value_ref]);
                central.scanForPeripheralsWithServices_options(None, Some(&options));
            }
            self.ivars().scanning.set(true);
        }

        fn stop_scan(&self, central: &CBCentralManager) {
            unsafe { central.stopScan() };
            self.ivars().scanning.set(false);
        }

        fn select(
            &self,
            central: &CBCentralManager,
            device_id: &str,
            name: Option<&str>,
        ) -> Result<(), BleLockError> {
            if device_id.trim().is_empty() {
                self.clear(central);
                return Err(BleLockError::device_not_selected());
            }
            self.cancel(central);
            *self.ivars().selected_id.borrow_mut() = Some(device_id.to_string());
            *self.ivars().selected_name.borrow_mut() = name.map(|value| value.to_string());
            if self.ivars().mode.get() == BleLockMode::Active {
                self.connect_selected(central, device_id)?;
            }
            Ok(())
        }

        fn clear(&self, central: &CBCentralManager) {
            self.cancel(central);
            *self.ivars().selected_id.borrow_mut() = None;
            *self.ivars().selected_name.borrow_mut() = None;
        }

        fn set_mode(&self, central: &CBCentralManager, mode: BleLockMode) {
            let previous = self.ivars().mode.replace(mode);
            if previous == mode {
                return;
            }
            match mode {
                BleLockMode::Passive => self.cancel(central),
                BleLockMode::Active => {
                    let selected = self.ivars().selected_id.borrow().clone();
                    if let Some(id) = selected {
                        if let Err(error) = self.connect_selected(central, &id) {
                            self.emit(BleCentralEvent::Error(error));
                        }
                    }
                }
            }
        }

        fn request_rssi(&self) {
            if self.ivars().mode.get() != BleLockMode::Active {
                return;
            }
            let peripheral = self.ivars().peripheral.borrow();
            if let Some(peripheral) = peripheral.as_ref() {
                unsafe { peripheral.readRSSI() };
            }
        }

        fn current_state(&self, central: &CBCentralManager) -> BleBluetoothState {
            state_from(unsafe { central.state() })
        }

        fn connect_selected(
            &self,
            central: &CBCentralManager,
            device_id: &str,
        ) -> Result<(), BleLockError> {
            let peripheral = self
                .ivars()
                .peripherals
                .borrow()
                .get(device_id)
                .cloned()
                .or_else(|| retrieve_peripheral(central, device_id))
                .ok_or_else(BleLockError::device_unavailable)?;
            unsafe {
                peripheral.setDelegate(Some(ProtocolObject::from_ref(self)));
                central.connectPeripheral_options(&peripheral, None);
            }
            *self.ivars().peripheral.borrow_mut() = Some(peripheral);
            Ok(())
        }

        fn cancel(&self, central: &CBCentralManager) {
            let peripheral = self.ivars().peripheral.borrow_mut().take();
            if let Some(peripheral) = peripheral {
                unsafe {
                    peripheral.setDelegate(None);
                    central.cancelPeripheralConnection(&peripheral);
                }
            }
        }
    }

    fn new_delegate(state: DelegateState) -> Retained<CentralDelegate> {
        let this = CentralDelegate::alloc().set_ivars(state);
        unsafe { msg_send![super(this), init] }
    }

    fn peripheral_id(peripheral: &CBPeripheral) -> String {
        unsafe { peripheral.identifier().UUIDString() }.to_string()
    }

    fn string_from(object: &AnyObject) -> Option<String> {
        object
            .downcast_ref::<NSString>()
            .map(|value| value.to_string())
    }

    fn retrieve_peripheral(
        central: &CBCentralManager,
        device_id: &str,
    ) -> Option<Retained<CBPeripheral>> {
        let string = NSString::from_str(device_id);
        let identifier = NSUUID::initWithUUIDString(NSUUID::alloc(), &string)?;
        let identifiers = NSArray::from_retained_slice(&[identifier]);
        let found = unsafe { central.retrievePeripheralsWithIdentifiers(&identifiers) };
        found.firstObject()
    }

    fn state_from(state: CBManagerState) -> BleBluetoothState {
        if state == CBManagerState::Unknown {
            BleBluetoothState::Unknown
        } else if state == CBManagerState::Resetting {
            BleBluetoothState::Resetting
        } else if state == CBManagerState::Unsupported {
            BleBluetoothState::Unsupported
        } else if state == CBManagerState::Unauthorized {
            BleBluetoothState::Unauthorized
        } else if state == CBManagerState::PoweredOff {
            BleBluetoothState::PoweredOff
        } else {
            BleBluetoothState::PoweredOn
        }
    }

    fn authorization_from(authorization: CBManagerAuthorization) -> BleBluetoothAuthorization {
        if authorization == CBManagerAuthorization::NotDetermined {
            BleBluetoothAuthorization::NotDetermined
        } else if authorization == CBManagerAuthorization::Restricted {
            BleBluetoothAuthorization::Restricted
        } else if authorization == CBManagerAuthorization::Denied {
            BleBluetoothAuthorization::Denied
        } else {
            BleBluetoothAuthorization::AllowedAlways
        }
    }

    /// Main-thread-confined CoreBluetooth bridge.
    ///
    /// `BleCentral` is intentionally `!Send`/`!Sync`: all CoreBluetooth objects
    /// (and therefore this handle) stay on the main queue.
    pub struct BleCentral {
        manager: Retained<CBCentralManager>,
        delegate: Retained<CentralDelegate>,
    }

    impl BleCentral {
        /// Create the bridge on the main thread; callbacks are delivered on the
        /// main dispatch queue.
        pub fn new(handler: impl Fn(BleCentralEvent) + 'static) -> Self {
            let delegate = new_delegate(DelegateState {
                handler: Box::new(handler),
                mode: Cell::new(BleLockMode::Active),
                scanning: Cell::new(false),
                selected_id: RefCell::new(None),
                selected_name: RefCell::new(None),
                peripheral: RefCell::new(None),
                peripherals: RefCell::new(HashMap::new()),
                devices: RefCell::new(HashMap::new()),
            });
            let manager = unsafe {
                CBCentralManager::initWithDelegate_queue(
                    CBCentralManager::alloc(),
                    Some(ProtocolObject::from_ref(&*delegate)),
                    None,
                )
            };
            Self { manager, delegate }
        }

        /// Start scanning with duplicate advertisements allowed.
        pub fn start_scan(&self) {
            self.delegate.begin_scan(&self.manager);
        }

        /// Stop scanning.
        pub fn stop_scan(&self) {
            self.delegate.stop_scan(&self.manager);
        }

        /// Whether a scan is currently running.
        pub fn is_scanning(&self) -> bool {
            self.delegate.ivars().scanning.get()
        }

        /// Select exactly one device UUID, disconnecting any previous device.
        pub fn select_device(
            &self,
            device_id: &str,
            name: Option<&str>,
        ) -> Result<(), BleLockError> {
            self.delegate.select(&self.manager, device_id, name)
        }

        /// Clear the selection and cancel any connection.
        pub fn clear_device(&self) {
            self.delegate.clear(&self.manager);
        }

        /// Change active/passive mode.
        pub fn set_mode(&self, mode: BleLockMode) {
            self.delegate.set_mode(&self.manager, mode);
        }

        /// Request one RSSI read from the connected peripheral (active mode).
        ///
        /// Step 2's process-owned runtime drives the periodic cadence.
        pub fn request_rssi(&self) {
            self.delegate.request_rssi();
        }

        /// Current Bluetooth power state.
        pub fn state(&self) -> BleBluetoothState {
            self.delegate.current_state(&self.manager)
        }

        /// Bluetooth authorization status, queryable before allocating a manager.
        pub fn authorization() -> BleBluetoothAuthorization {
            authorization_from(unsafe { CBManager::authorization_class() })
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use super::{BleBluetoothAuthorization, BleCentralEvent};
    use crate::ble_lock::config::{BleLockError, BleLockMode};
    use crate::ble_lock::state::BleBluetoothState;

    /// Clear unavailable implementation for non-macOS targets.
    pub struct BleCentral;

    impl BleCentral {
        /// Create an unavailable bridge; the handler is never invoked.
        pub fn new(_handler: impl Fn(BleCentralEvent) + 'static) -> Self {
            Self
        }

        /// No-op on unsupported platforms.
        pub fn start_scan(&self) {}

        /// No-op on unsupported platforms.
        pub fn stop_scan(&self) {}

        /// Always `false` on unsupported platforms.
        pub fn is_scanning(&self) -> bool {
            false
        }

        /// Always fails with `platform_unavailable`.
        pub fn select_device(
            &self,
            _device_id: &str,
            _name: Option<&str>,
        ) -> Result<(), BleLockError> {
            Err(BleLockError::platform_unavailable())
        }

        /// No-op on unsupported platforms.
        pub fn clear_device(&self) {}

        /// No-op on unsupported platforms.
        pub fn set_mode(&self, _mode: BleLockMode) {}

        /// No-op on unsupported platforms.
        pub fn request_rssi(&self) {}

        /// Always unsupported on non-macOS targets.
        pub fn state(&self) -> BleBluetoothState {
            BleBluetoothState::Unsupported
        }

        /// Authorization is never granted on non-macOS targets.
        pub fn authorization() -> BleBluetoothAuthorization {
            BleBluetoothAuthorization::NotDetermined
        }
    }
}

pub use imp::BleCentral;
