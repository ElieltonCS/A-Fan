use crate::core::{self, Telemetry};
use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use windows::{
    Win32::System::{
        Com::*,
        Variant::{VARIANT, VariantToString},
        Wmi::*,
    },
    core::{BSTR, PCWSTR},
};

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
static WRITE_COUNT: AtomicU64 = AtomicU64::new(0);
pub fn write_count() -> u64 {
    WRITE_COUNT.load(Ordering::Relaxed)
}
fn text(object: &IWbemClassObject, name: &str) -> Result<String, String> {
    let mut value = VARIANT::default();
    let name = wide(name);
    unsafe { object.Get(PCWSTR(name.as_ptr()), 0, &mut value, None, None) }
        .map_err(|e| e.to_string())?;
    let mut buffer = [0u16; 1024];
    unsafe { VariantToString(&value, &mut buffer) }.map_err(|e| e.to_string())?;
    let length = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Ok(String::from_utf16_lossy(&buffer[..length]))
}
struct Apartment;
impl Apartment {
    fn new() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|e| e.to_string())?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() }
    }
}

struct Wmi {
    service: IWbemServices,
    _apartment: Apartment,
}
impl Wmi {
    fn connect(namespace: &str) -> Result<Self, String> {
        let apartment = Apartment::new()?;
        let locator: IWbemLocator =
            unsafe { CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER) }
                .map_err(|e| e.to_string())?;
        let empty = BSTR::new();
        let service = unsafe {
            locator.ConnectServer(
                &BSTR::from(namespace),
                &empty,
                &empty,
                &empty,
                0,
                &empty,
                None,
            )
        }
        .map_err(|e| e.to_string())?;
        unsafe {
            CoSetProxyBlanket(
                &service,
                10,
                0,
                PCWSTR::null(),
                RPC_C_AUTHN_LEVEL_CALL,
                RPC_C_IMP_LEVEL_IMPERSONATE,
                None,
                EOAC_NONE,
            )
        }
        .map_err(|e| e.to_string())?;
        Ok(Self {
            service,
            _apartment: apartment,
        })
    }
    fn first(&self, class: &str, columns: &str) -> Result<IWbemClassObject, String> {
        let rows = unsafe {
            self.service.ExecQuery(
                &BSTR::from("WQL"),
                &BSTR::from(format!("SELECT {columns} FROM {class}")),
                WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY,
                None,
            )
        }
        .map_err(|e| e.to_string())?;
        for _ in 0..3 {
            let mut objects = [None];
            let mut count = 0;
            let status = unsafe { rows.Next(5000, &mut objects, &mut count) };
            status.ok().map_err(|e| e.to_string())?;
            if let Some(object) = objects[0].take() {
                return Ok(object);
            }
            if status.0 != WBEM_S_TIMEDOUT.0 {
                return Err(format!(
                    "{class}: nenhuma instância retornada pelo provedor WMI."
                ));
            }
        }
        Err(format!(
            "Tempo excedido aguardando o provedor WMI de {class}."
        ))
    }
    fn call(
        &self,
        instance: &IWbemClassObject,
        method: &str,
        input: u64,
        setter: bool,
    ) -> Result<u64, String> {
        let mut class = None;
        unsafe {
            self.service.GetObject(
                &BSTR::from("AcerGamingFunction"),
                WBEM_GENERIC_FLAG_TYPE(0),
                None,
                Some(&mut class),
                None,
            )
        }
        .map_err(|e| e.to_string())?;
        let mut signature = None;
        let mut output_signature = None;
        let method_wide = wide(method);
        unsafe {
            class.ok_or("Classe WMI ausente")?.GetMethod(
                PCWSTR(method_wide.as_ptr()),
                0,
                &mut signature,
                &mut output_signature,
            )
        }
        .map_err(|e| e.to_string())?;
        let parameters = unsafe { signature.ok_or("Assinatura WMI ausente")?.SpawnInstance(0) }
            .map_err(|e| e.to_string())?;
        let value = if setter {
            VARIANT::from(input.to_string().as_str())
        } else {
            VARIANT::from(input as u32 as i32)
        };
        unsafe { parameters.Put(windows::core::w!("gmInput"), 0, &value, 0) }
            .map_err(|e| e.to_string())?;
        let path = text(instance, "__PATH")?;
        let mut output = None;
        unsafe {
            self.service.ExecMethod(
                &BSTR::from(path),
                &BSTR::from(method),
                WBEM_GENERIC_FLAG_TYPE(0),
                None,
                &parameters,
                Some(&mut output),
                None,
            )
        }
        .map_err(|e| e.to_string())?;
        text(
            &output.ok_or("O firmware não devolveu resposta")?,
            "gmOutput",
        )?
        .parse::<u64>()
        .map_err(|e| format!("Resposta WMI inválida: {e}"))
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Identity {
    pub model: String,
    pub bios: String,
    pub cpu: String,
}
pub struct Device {
    instance: IWbemClassObject,
    wmi: Wmi,
    pub identity: Identity,
    pub supported_sensors: u16,
    pub controls_available: bool,
    pub reason: String,
    pub behavior_raw: Option<u64>,
    pub speed_raw: Option<u64>,
    changed: bool,
}
impl Device {
    pub fn connect() -> Result<Self, String> {
        let system = Wmi::connect("ROOT\\CIMV2")?;
        let identity = Identity {
            model: text(&system.first("Win32_ComputerSystem", "Model")?, "Model")?,
            bios: text(
                &system.first("Win32_BIOS", "SMBIOSBIOSVersion")?,
                "SMBIOSBIOSVersion",
            )?,
            cpu: text(&system.first("Win32_Processor", "Name")?, "Name")?,
        };
        if identity.model != "Aspire A515-45" {
            return Err(format!(
                "Modelo {} ainda não validado para o A-Fan.",
                identity.model
            ));
        }
        let wmi = Wmi::connect("ROOT\\WMI")?;
        let instance = wmi.first("AcerGamingFunction", "*")?;
        let supported = core::status_ok(wmi.call(&instance, "GetGamingSysInfo", 0, false)?)?;
        let supported_sensors = ((supported >> 24) & 0xffff) as u16;
        if supported_sensors & 3 != 3 {
            return Err(
                "O firmware não expõe temperatura e RPM necessários para controle seguro.".into(),
            );
        }
        let behavior_raw = wmi.call(&instance, "GetGamingFanBehavior", 1, false).ok();
        let speed_raw = wmi.call(&instance, "GetGamingFanSpeed", 1, false).ok();
        let controls_available = behavior_raw
            .zip(speed_raw)
            .is_some_and(|(b, s)| core::decode_control(b, s).is_ok());
        let reason = if controls_available {
            "Controle independente disponível; calibração necessária.".into()
        } else {
            "O firmware não confirmou controle independente da ventoinha. Monitoramento ativo; aplicação de curvas bloqueada.".into()
        };
        Ok(Self {
            instance,
            wmi,
            identity,
            supported_sensors,
            controls_available,
            reason,
            behavior_raw,
            speed_raw,
            changed: false,
        })
    }
    fn raw(&self, method: &str, input: u64, setter: bool) -> Result<u64, String> {
        if setter {
            WRITE_COUNT.fetch_add(1, Ordering::Relaxed);
        }
        self.wmi.call(&self.instance, method, input, setter)
    }
    pub fn read(&self) -> Result<Telemetry, String> {
        let temperature = f64::from(core::sensor_value(self.raw(
            "GetGamingSysInfo",
            u64::from(core::sensor_input(1)),
            false,
        )?)?);
        let rpm = u32::from(core::sensor_value(self.raw(
            "GetGamingSysInfo",
            u64::from(core::sensor_input(2)),
            false,
        )?)?);
        let mut value = Telemetry {
            temperature: Some(temperature),
            rpm: Some(rpm),
            controls_available: self.controls_available,
            reason: self.reason.clone(),
            ..Default::default()
        };
        if !value.temperature_valid() {
            return Err("Sensor de temperatura inválido; controle personalizado suspenso.".into());
        }
        if self.controls_available {
            let (mode, speed) = core::decode_control(
                self.raw("GetGamingFanBehavior", 1, false)?,
                self.raw("GetGamingFanSpeed", 1, false)?,
            )?;
            value.fan_mode = Some(mode);
            value.fan_percent = Some(speed);
        }
        Ok(value)
    }
    pub fn restore(&mut self) -> Result<(), String> {
        if !self.changed {
            return Ok(());
        }
        let command = self
            .raw("SetGamingFanBehavior", core::fan_behavior_input(1), true)
            .and_then(core::status_ok);
        let readback = self
            .raw("GetGamingFanBehavior", 1, false)
            .and_then(core::status_ok);
        if readback.is_ok_and(|r| (r >> 8) & 3 == 1) {
            self.changed = false;
            return Ok(());
        }
        let maximum = self.set_percent(100);
        Err(format!(
            "Retorno ao automático Acer não confirmado. {} {}",
            command.err().unwrap_or_default(),
            match maximum {
                Ok(()) => "Ventoinha mantida em 100% por segurança.".into(),
                Err(e) => format!("Também não foi possível confirmar 100%: {e}"),
            }
        ))
    }
    pub fn force_restore(&mut self) -> Result<(), String> {
        if !self.controls_available {
            return Err(self.reason.clone());
        }
        self.changed = true;
        self.restore()
    }
    pub fn set_percent(&mut self, percent: u8) -> Result<(), String> {
        if !self.controls_available {
            return Err(self.reason.clone());
        }
        let input = core::fan_speed_input(percent)?;
        self.changed = true;
        core::status_ok(self.raw("SetGamingFanSpeed", input, true)?)?;
        core::status_ok(self.raw("SetGamingFanBehavior", core::fan_behavior_input(3), true)?)?;
        let readback = self.read()?;
        if readback.fan_mode != Some(3) || readback.fan_percent != Some(percent) {
            return Err(
                "Outro controlador ou firmware não confirmou a velocidade solicitada.".into(),
            );
        }
        Ok(())
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

pub fn diagnostic() -> serde_json::Value {
    match Device::connect() {
        Ok(device) => {
            serde_json::json!({"version":1,"identity":device.identity,"supported_sensors":device.supported_sensors,
            "behavior_raw":device.behavior_raw.map(|v|format!("0x{v:016X}")),"speed_raw":device.speed_raw.map(|v|format!("0x{v:016X}")),
            "telemetry":device.read().map_err(|e|e.to_string()),"hardware_writes":write_count()})
        }
        Err(error) => {
            serde_json::json!({"version":1,"error":error,"hardware_writes":write_count()})
        }
    }
}
