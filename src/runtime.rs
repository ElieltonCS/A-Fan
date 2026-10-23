use crate::{
    core::{self, Calibration, Controller, Profile, Settings, Telemetry},
    hardware::{Device, Identity},
    platform::{self, OwnedHandle},
    storage,
};
use std::{
    os::windows::process::CommandExt,
    process::{Child, Command},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{Foundation::*, System::Threading::*},
    core::PWSTR,
};

#[derive(Clone)]
pub struct View {
    pub identity: Option<Identity>,
    pub telemetry: Telemetry,
    pub message: String,
    pub active: Profile,
    pub calibrating: bool,
    pub calibrated: bool,
    pub guard_ready: bool,
    pub exit_ready: bool,
    pub last_sample: Option<Instant>,
    pub settings: Settings,
}
pub enum Request {
    Apply(Settings),
    SaveDraft(Settings),
    Calibrate,
    Firmware,
    Suspend,
    Resume,
    Exit,
    Diagnostic,
}
pub struct Runtime {
    pub state: Arc<Mutex<View>>,
    pub sender: Sender<Request>,
}
impl Runtime {
    pub fn start(read_only: bool) -> Self {
        let (settings, message) = match storage::load() {
            Ok(s) => (s, "Conectando aos sensores…".into()),
            Err(e) => (Settings::default(), e),
        };
        let state = Arc::new(Mutex::new(View {
            identity: None,
            telemetry: Telemetry::default(),
            message,
            active: Profile::Firmware,
            calibrating: false,
            calibrated: false,
            guard_ready: false,
            exit_ready: false,
            last_sample: None,
            settings,
        }));
        let (sender, receiver) = mpsc::channel();
        let worker_state = state.clone();
        thread::spawn(move || worker(worker_state, receiver, read_only));
        Self { state, sender }
    }
    pub fn view(&self) -> View {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
    pub fn send(&self, request: Request) {
        let _ = self.sender.send(request);
    }
}

struct Guard {
    child: Child,
    beat: OwnedHandle,
    stop: OwnedHandle,
    recover: OwnedHandle,
    recovered: OwnedHandle,
}
impl Guard {
    fn start() -> Result<Self, String> {
        let pid = std::process::id();
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let prefix = format!("Local\\A-Fan-{pid}-{nonce}");
        let beat = platform::event(&format!("{prefix}-beat"))?;
        let stop = platform::event(&format!("{prefix}-stop"))?;
        let ready = platform::event(&format!("{prefix}-ready"))?;
        let recover = platform::event(&format!("{prefix}-recover"))?;
        let recovered = platform::event(&format!("{prefix}-recovered"))?;
        let mut child = Command::new(std::env::current_exe().map_err(|e| e.to_string())?)
            .args(["--guard", &pid.to_string(), &prefix])
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn()
            .map_err(|e| e.to_string())?;
        if unsafe { WaitForSingleObject(ready.0, 8000) } != WAIT_OBJECT_0 {
            let _ = platform::signal(&stop);
            let _ = child.kill();
            let _ = child.wait();
            return Err(
                "Processo de recuperação não confirmou prontidão. Nenhuma redução foi aplicada."
                    .into(),
            );
        }
        Ok(Self {
            child,
            beat,
            stop,
            recover,
            recovered,
        })
    }
    fn alive(&mut self) -> bool {
        self.child.try_wait().is_ok_and(|s| s.is_none())
    }
    fn beat(&self) -> Result<(), String> {
        platform::signal(&self.beat)
    }
    fn recovery_check(&self) -> Result<(), String> {
        platform::signal(&self.recover)
    }
    fn stop(mut self) {
        let _ = platform::signal(&self.stop);
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.alive() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        if self.alive() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

pub fn guard(args: &[String]) -> Result<(), String> {
    if args.len() != 2 {
        return Err("Argumentos de recuperação inválidos.".into());
    }
    let pid = args[0].parse::<u32>().map_err(|e| e.to_string())?;
    if !args[1].starts_with(&format!("Local\\A-Fan-{pid}-")) {
        return Err("Identidade de recuperação inválida.".into());
    }
    let beat = platform::open_event(&format!("{}-beat", args[1]))?;
    let stop = platform::open_event(&format!("{}-stop", args[1]))?;
    let ready = platform::open_event(&format!("{}-ready", args[1]))?;
    let recover = platform::open_event(&format!("{}-recover", args[1]))?;
    let recovered = platform::open_event(&format!("{}-recovered", args[1]))?;
    let parent = OwnedHandle(
        unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE,
                false,
                pid,
            )
        }
        .map_err(|e| e.to_string())?,
    );
    let mut image = [0u16; 32768];
    let mut length = image.len() as u32;
    unsafe {
        QueryFullProcessImageNameW(
            parent.0,
            PROCESS_NAME_WIN32,
            PWSTR(image.as_mut_ptr()),
            &mut length,
        )
    }
    .map_err(|e| e.to_string())?;
    let image = std::path::PathBuf::from(String::from_utf16_lossy(&image[..length as usize]));
    if image != std::env::current_exe().map_err(|e| e.to_string())? {
        return Err("O processo observado não é o A-Fan.".into());
    }
    let mut device = Device::connect()?;
    if !device.controls_available {
        return Err(device.reason.clone());
    }
    platform::signal(&ready)?;
    let mut last_beat = Instant::now();
    loop {
        if platform::signaled(&stop) {
            return Ok(());
        }
        if platform::signaled(&beat) {
            last_beat = Instant::now();
        }
        if platform::signaled(&recover) {
            device.force_restore()?;
            platform::signal(&recovered)?;
        }
        if unsafe { WaitForSingleObject(parent.0, 0) } == WAIT_OBJECT_0
            || last_beat.elapsed() >= core::STALE_AFTER
        {
            if unsafe { WaitForSingleObject(parent.0, 0) } != WAIT_OBJECT_0 {
                unsafe { TerminateProcess(parent.0, 2) }.map_err(|e| e.to_string())?;
            }
            let result = device.force_restore();
            let _ = storage::write_json(
                &storage::data_dir().join("recovery.json"),
                &serde_json::json!({"restored":result.is_ok(),"error":result.as_ref().err()}),
            );
            return result;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalibrationPhase {
    Cooling,
    RecoveryBeforeReduction,
    Reducing,
    RecoveryAtEnd,
}
struct CalibrationRun {
    deadline: Instant,
    step_since: Instant,
    percent: u8,
    minimum: u8,
    phase: CalibrationPhase,
}
impl CalibrationRun {
    fn recovery_confirmed(&mut self) -> Result<bool, String> {
        match self.phase {
            CalibrationPhase::RecoveryBeforeReduction => {
                self.phase = CalibrationPhase::Reducing;
                Ok(false)
            }
            CalibrationPhase::RecoveryAtEnd => Ok(true),
            _ => Err("Confirmação de recuperação fora da etapa esperada.".into()),
        }
    }
}
fn update(state: &Arc<Mutex<View>>, f: impl FnOnce(&mut View)) {
    f(&mut state.lock().unwrap_or_else(|e| e.into_inner()));
}
fn stop_control(
    device: &mut Device,
    guard: &mut Option<Guard>,
    controller: &mut Controller,
) -> Result<(), String> {
    device.restore()?;
    if let Some(guard) = guard.take() {
        guard.stop();
    }
    controller.reset();
    Ok(())
}

fn worker(state: Arc<Mutex<View>>, receiver: Receiver<Request>, read_only: bool) {
    let mut device = loop {
        match Device::connect() {
            Ok(d) => break d,
            Err(error) => {
                update(&state, |s| {
                    s.message = format!("Sensores indisponíveis: {error}")
                });
                if let Ok(request) = receiver.recv_timeout(Duration::from_secs(5)) {
                    match request {
                        Request::Exit => {
                            update(&state, |s| s.exit_ready = true);
                            return;
                        }
                        Request::Apply(_) => {
                            update(&state, |s| {
                                s.message = "Aplicação bloqueada: sensores indisponíveis.".into()
                            });
                        }
                        Request::SaveDraft(draft) => {
                            let mut saved = state
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .settings
                                .clone();
                            saved.custom_curve = draft.custom_curve;
                            match storage::save(&saved) {
                                Ok(()) => update(&state, |s| s.settings = saved),
                                Err(e) => update(&state, |s| s.message = e),
                            }
                        }
                        Request::Diagnostic => {
                            let _ = storage::write_json(
                                &storage::data_dir().join("diagnostic.json"),
                                &crate::hardware::diagnostic(),
                            );
                        }
                        _ => {}
                    }
                }
            }
        }
    };
    let mut settings = state
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .settings
        .clone();
    let calibrated = !read_only
        && settings
            .calibration
            .as_ref()
            .is_some_and(|c| c.valid_for(&device.identity.model, &device.identity.bios));
    update(&state, |s| {
        s.identity = Some(device.identity.clone());
        s.calibrated = calibrated;
        s.message = if calibrated && device.controls_available {
            "Controle e calibração validados. A fan seguirá a curva selecionada.".into()
        } else {
            device.reason.clone()
        };
    });
    let mut guard: Option<Guard> = None;
    let mut controller = Controller::new();
    let mut calibration: Option<CalibrationRun> = None;
    let mut active = false;
    let mut suspended = false;
    let mut good_samples = 0u8;
    let mut resume_desired = calibrated && settings.profile != Profile::Firmware;
    let mut applied_curve = settings.curve();
    let mut applied_profile = settings.profile;
    let mut next_sample = Instant::now();
    loop {
        let timeout = next_sample
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(1));
        let request = match receiver.recv_timeout(timeout) {
            Ok(r) => Some(r),
            Err(mpsc::RecvTimeoutError::Timeout) => None,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                let _ = stop_control(&mut device, &mut guard, &mut controller);
                break;
            }
        };
        if let Some(request) = request {
            let outcome: Result<(), String> = (|| {
                match request {
                    Request::Exit => {
                        stop_control(&mut device, &mut guard, &mut controller)?;
                        update(&state, |s| s.exit_ready = true);
                        return Ok(());
                    }
                    Request::Suspend => {
                        stop_control(&mut device, &mut guard, &mut controller)?;
                        suspended = true;
                        active = false;
                        good_samples = 0;
                        resume_desired = !read_only
                            && settings.profile != Profile::Firmware
                            && settings.calibration.as_ref().is_some_and(|c| {
                                c.valid_for(&device.identity.model, &device.identity.bios)
                            });
                        calibration = None;
                    }
                    Request::Resume => {
                        suspended = false;
                        good_samples = 0;
                        next_sample = Instant::now();
                    }
                    Request::Firmware => {
                        stop_control(&mut device, &mut guard, &mut controller)?;
                        active = false;
                        resume_desired = false;
                        calibration = None;
                        settings.profile = Profile::Firmware;
                        storage::save(&settings)?;
                    }
                    Request::Diagnostic => {
                        storage::write_json(
                            &storage::data_dir().join("diagnostic.json"),
                            &crate::hardware::diagnostic(),
                        )?;
                        update(&state, |s| {
                            s.message = format!(
                                "Diagnóstico salvo em {}",
                                storage::data_dir().join("diagnostic.json").display()
                            )
                        });
                    }
                    Request::SaveDraft(draft) => {
                        let mut saved = settings.clone();
                        saved.custom_curve = draft.custom_curve;
                        saved.validate()?;
                        storage::save(&saved)?;
                        settings = saved;
                    }
                    Request::Apply(new_settings) => {
                        new_settings.validate()?;
                        if read_only && new_settings.profile != Profile::Firmware {
                            return Err(
                                "Teste integrado: comandos de alteração de fan desabilitados."
                                    .into(),
                            );
                        }
                        if new_settings.profile == Profile::Firmware {
                            stop_control(&mut device, &mut guard, &mut controller)?;
                            active = false;
                            resume_desired = false;
                        } else {
                            if !device.controls_available {
                                return Err(device.reason.clone());
                            }
                            if !new_settings.calibration.as_ref().is_some_and(|c| {
                                c.valid_for(&device.identity.model, &device.identity.bios)
                            }) {
                                return Err(
                                    "Calibração não validada para este notebook e BIOS.".into()
                                );
                            }
                            if good_samples < 3 {
                                return Err("Aguarde três leituras válidas dos sensores.".into());
                            }
                            if guard.is_none() {
                                guard = Some(Guard::start()?);
                            }
                            active = true;
                            applied_curve = new_settings.curve();
                            applied_profile = new_settings.profile;
                            controller.reset();
                            resume_desired = false;
                        }
                        storage::save(&new_settings)?;
                        settings = new_settings;
                    }
                    Request::Calibrate => {
                        if read_only {
                            return Err("Teste integrado: calibração física desabilitada.".into());
                        }
                        if !device.controls_available {
                            return Err(device.reason.clone());
                        }
                        if calibration.is_some() {
                            return Err("Uma calibração já está em andamento.".into());
                        }
                        if good_samples < 3 {
                            return Err("Aguarde os sensores estabilizarem.".into());
                        }
                        stop_control(&mut device, &mut guard, &mut controller)?;
                        guard = Some(Guard::start()?);
                        guard.as_ref().unwrap().beat()?;
                        device.force_restore()?;
                        device.set_percent(100)?;
                        active = false;
                        resume_desired = false;
                        calibration = Some(CalibrationRun {
                            deadline: Instant::now() + Duration::from_secs(180),
                            step_since: Instant::now(),
                            percent: 100,
                            minimum: 100,
                            phase: CalibrationPhase::Cooling,
                        });
                    }
                }
                Ok(())
            })();
            if let Err(error) = outcome {
                let recovery = stop_control(&mut device, &mut guard, &mut controller);
                active = false;
                resume_desired = false;
                calibration = None;
                update(&state, |s| {
                    s.message = match recovery {
                        Ok(()) => error,
                        Err(e) => format!("{error} Retorno à Acer NÃO confirmado: {e}"),
                    }
                });
            }
            update(&state, |s| {
                s.settings = settings.clone();
                s.calibrating = calibration.is_some();
                s.guard_ready = guard.is_some();
                s.active = if active {
                    applied_profile
                } else {
                    Profile::Firmware
                };
            });
            if state.lock().unwrap_or_else(|e| e.into_inner()).exit_ready {
                break;
            }
        }
        if Instant::now() < next_sample {
            continue;
        }
        next_sample = Instant::now() + Duration::from_secs(1);
        if suspended {
            continue;
        }
        let sample = (|| -> Result<Telemetry, String> {
            if let Some(guard) = guard.as_mut() {
                if !guard.alive() {
                    return Err("Processo de recuperação indisponível; controle suspenso.".into());
                }
                guard.beat()?;
            }
            let sample = device.read()?;
            good_samples = good_samples.saturating_add(1);
            if !read_only && resume_desired && good_samples >= 3 && device.controls_available {
                guard = Some(Guard::start()?);
                applied_curve = settings.curve();
                applied_profile = settings.profile;
                active = true;
                resume_desired = false;
                controller.reset();
            }
            if let Some(run) = calibration.as_mut() {
                if Instant::now() >= run.deadline {
                    return Err(
                        "Calibração interrompida por tempo excedido. Aguarde o notebook esfriar."
                            .into(),
                    );
                }
                let temperature = sample.temperature.unwrap();
                if temperature >= core::MAX_TEMP {
                    device.set_percent(100)?;
                    return Err("Calibração interrompida pela guarda térmica.".into());
                }
                if matches!(
                    run.phase,
                    CalibrationPhase::RecoveryBeforeReduction | CalibrationPhase::RecoveryAtEnd
                ) {
                    if platform::signaled(&guard.as_ref().unwrap().recovered) {
                        if sample.fan_mode != Some(1) {
                            return Err(
                                "Recuperação independente não confirmou modo automático.".into()
                            );
                        }
                        if !run.recovery_confirmed()? {
                            device.set_percent(100)?;
                            run.step_since = Instant::now();
                        } else {
                            let verified = Calibration {
                                model: device.identity.model.clone(),
                                bios: device.identity.bios.clone(),
                                minimum_percent: run.minimum.saturating_add(10).clamp(30, 100),
                                version: 1,
                                temperature_and_rpm_verified: true,
                                independent_speed_verified: true,
                                recovery_verified: true,
                            };
                            device.force_restore()?;
                            settings.calibration = Some(verified);
                            settings.profile = Profile::Auto;
                            let floor = settings.floor();
                            for point in &mut settings.custom_curve.points {
                                point.percent = point.percent.max(floor);
                            }
                            storage::save(&settings)?;
                            calibration = None;
                            resume_desired = true;
                            update(&state, |s| {
                                s.calibrated = true;
                                s.settings = settings.clone();
                                s.message = "Calibração concluída. Auto habilitado.".into();
                            });
                        }
                    } else if run.step_since.elapsed() > core::STALE_AFTER {
                        return Err("Recuperação independente não confirmou retorno à Acer.".into());
                    }
                } else {
                    if sample.fan_mode != Some(3) || sample.fan_percent != Some(run.percent) {
                        return Err("O firmware ou outro controlador alterou a velocidade durante a calibração.".into());
                    }
                    if sample.rpm.unwrap_or(0) == 0 {
                        device.set_percent(100)?;
                        return Err("RPM zerado durante a calibração; redução interrompida.".into());
                    }
                    if run.phase == CalibrationPhase::Cooling {
                        if temperature <= 60. && run.step_since.elapsed() >= Duration::from_secs(10)
                        {
                            // Verify independent recovery from Custom 100% before any reduction.
                            guard.as_ref().unwrap().recovery_check()?;
                            run.phase = CalibrationPhase::RecoveryBeforeReduction;
                            run.step_since = Instant::now();
                        }
                    } else {
                        if temperature > 65. {
                            return Err("Notebook acima de 65°C durante a busca do mínimo. Aguarde esfriar e repita.".into());
                        }
                        if run.step_since.elapsed() >= Duration::from_secs(10) {
                            run.minimum = run.percent;
                            if run.percent == 30 {
                                guard.as_ref().unwrap().recovery_check()?;
                                run.phase = CalibrationPhase::RecoveryAtEnd;
                                run.step_since = Instant::now();
                            } else {
                                run.percent -= 10;
                                device.set_percent(run.percent)?;
                                run.step_since = Instant::now();
                            }
                        }
                    }
                    update(&state, |s| {
                        s.message = if run.phase == CalibrationPhase::Cooling {
                            "Calibrando: refrigeração máxima até a CPU ficar abaixo de 60°C…".into()
                        } else {
                            format!(
                                "Calibrando: {}%. Não feche o notebook durante o teste.",
                                run.percent
                            )
                        }
                    });
                }
            } else if active {
                if sample.rpm == Some(0) {
                    return Err(
                        "RPM zerado em controle personalizado; retorno à Acer solicitado.".into(),
                    );
                }
                if controller
                    .commanded()
                    .is_some_and(|p| sample.fan_mode != Some(3) || sample.fan_percent != Some(p))
                {
                    return Err(
                        "Outro controlador alterou a ventoinha. Controle personalizado suspenso."
                            .into(),
                    );
                }
                let target =
                    controller.next(&applied_curve, sample.temperature.unwrap(), Instant::now());
                if sample.fan_mode != Some(3) || sample.fan_percent != Some(target) {
                    device.set_percent(target)?;
                }
            }
            Ok(sample)
        })();
        match sample {
            Ok(sample) => update(&state, |s| {
                s.telemetry = sample;
                s.last_sample = Some(Instant::now());
                s.calibrating = calibration.is_some();
                s.guard_ready = guard.is_some();
                s.active = if active {
                    applied_profile
                } else {
                    Profile::Firmware
                };
            }),
            Err(error) => {
                good_samples = 0;
                let restore = stop_control(&mut device, &mut guard, &mut controller);
                active = false;
                calibration = None;
                resume_desired = false;
                update(&state, |s| {
                    s.telemetry = Telemetry::default();
                    s.last_sample = None;
                    s.calibrating = false;
                    s.guard_ready = guard.is_some();
                    s.active = Profile::Firmware;
                    s.message = match restore {
                        Ok(()) => error,
                        Err(e) => format!("{error} Retorno à Acer NÃO confirmado: {e}"),
                    };
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reductions_require_recovery_before_and_after_calibration() {
        let now = Instant::now();
        let mut run = CalibrationRun {
            deadline: now + Duration::from_secs(180),
            step_since: now,
            percent: 100,
            minimum: 100,
            phase: CalibrationPhase::Cooling,
        };
        assert!(run.recovery_confirmed().is_err());
        run.phase = CalibrationPhase::RecoveryBeforeReduction;
        assert!(!run.recovery_confirmed().unwrap());
        assert_eq!(run.phase, CalibrationPhase::Reducing);
        assert_eq!(run.percent, 100);
        assert!(run.recovery_confirmed().is_err());
        run.phase = CalibrationPhase::RecoveryAtEnd;
        assert!(run.recovery_confirmed().unwrap());
    }
}
