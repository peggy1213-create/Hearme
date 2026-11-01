//! Watches for default audio-endpoint changes via `IMMNotificationClient` and forwards
//! them to the session's control loop, which re-points the mic and emits
//! `device_changed`. Mirrors the macOS helper's `DeviceWatcher`.

use std::sync::mpsc::Sender;
use std::sync::Mutex;

use windows::core::{implement, PCWSTR};
use windows::Win32::Media::Audio::{
    eCapture, eConsole, eRender, EDataFlow, ERole, IMMDeviceEnumerator, IMMNotificationClient,
    IMMNotificationClient_Impl, MMDeviceEnumerator, DEVICE_STATE,
};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::UI::Shell::PropertiesSystem::PROPERTYKEY;

use crate::session::ControlMsg;

/// Keeps the endpoint-notification callback registered for its lifetime; unregisters on
/// drop. Must be created on a COM-initialized thread (the control thread).
pub struct DeviceWatcher {
    enumerator: IMMDeviceEnumerator,
    client: IMMNotificationClient,
}

impl DeviceWatcher {
    pub fn register(tx: Sender<ControlMsg>) -> windows::core::Result<DeviceWatcher> {
        let enumerator: IMMDeviceEnumerator =
            unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? };
        let client: IMMNotificationClient = Notifier { tx: Mutex::new(tx) }.into();
        unsafe { enumerator.RegisterEndpointNotificationCallback(&client)? };
        Ok(DeviceWatcher { enumerator, client })
    }
}

impl Drop for DeviceWatcher {
    fn drop(&mut self) {
        unsafe {
            let _ = self
                .enumerator
                .UnregisterEndpointNotificationCallback(&self.client);
        }
    }
}

/// The notification callback. Reacts only to the console default device changing, so a
/// single change doesn't fan out into one message per role.
#[implement(IMMNotificationClient)]
struct Notifier {
    tx: Mutex<Sender<ControlMsg>>,
}

impl IMMNotificationClient_Impl for Notifier_Impl {
    fn OnDefaultDeviceChanged(
        &self,
        flow: EDataFlow,
        role: ERole,
        _default_device_id: &PCWSTR,
    ) -> windows::core::Result<()> {
        if role == eConsole {
            let kind = if flow == eCapture {
                "input"
            } else if flow == eRender {
                "output"
            } else {
                return Ok(());
            };
            if let Ok(tx) = self.tx.lock() {
                let _ = tx.send(ControlMsg::DeviceChanged(kind));
            }
        }
        Ok(())
    }

    fn OnDeviceStateChanged(
        &self,
        _device_id: &PCWSTR,
        _new_state: DEVICE_STATE,
    ) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnDeviceAdded(&self, _device_id: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnDeviceRemoved(&self, _device_id: &PCWSTR) -> windows::core::Result<()> {
        Ok(())
    }

    fn OnPropertyValueChanged(
        &self,
        _device_id: &PCWSTR,
        _key: &PROPERTYKEY,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}
