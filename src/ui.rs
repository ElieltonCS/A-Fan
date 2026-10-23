use crate::{
    core::{Curve, MAX_TEMP, Profile, STALE_AFTER, Settings},
    hardware::wide,
    platform::{self, OwnedHandle},
    runtime::{Request, Runtime},
    storage,
};
use std::{
    cell::RefCell,
    mem::size_of,
    path::Path,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{LibraryLoader::GetModuleHandleW, Threading::*},
        UI::{
            Controls::*,
            HiDpi::*,
            Input::KeyboardAndMouse::{EnableWindow, IsWindowEnabled},
            Shell::*,
            WindowsAndMessaging::*,
        },
    },
    core::{PCWSTR, w},
};

const TRAY_MESSAGE: u32 = WM_APP + 1;
const TRAY_KEY_SELECT: u32 = NIN_SELECT | NINF_KEY;
const EXIT_MESSAGE: u32 = WM_APP + 2;
const ID_PROFILE: usize = 100;
const ID_APPLY: usize = 101;
const ID_CALIBRATE: usize = 102;
const ID_STARTUP: usize = 107;
const ID_DIAGNOSTIC: usize = 108;
const ID_FOLDER: usize = 109;
const ID_FIRMWARE: usize = 110;
const ID_EXIT: usize = 111;
const ID_OPEN: usize = 112;
const ID_MENU_PROFILE: usize = 300;

struct Smoke {
    path: String,
    started: Instant,
    stage: u8,
    initial_hidden: bool,
    minimize_hides: bool,
    presets_valid: bool,
    manual_controls_absent: bool,
    capture_error: Option<String>,
}
struct App {
    runtime: Runtime,
    settings: Settings,
    curve: Curve,
    selected: Profile,
    hwnd: HWND,
    combo: HWND,
    metrics: HWND,
    status: HWND,
    active: HWND,
    description: HWND,
    points: HWND,
    apply: HWND,
    calibrate: HWND,
    startup: HWND,
    footer: HWND,
    font: HFONT,
    metric_font: HFONT,
    icon: HICON,
    dpi: f64,
    loading: bool,
    taskbar_message: u32,
    last_tooltip: String,
    smoke: Option<Smoke>,
    tray_added: bool,
    heat_alert_shown: bool,
}
impl App {
    fn new(smoke: Option<String>) -> Self {
        let runtime = Runtime::start(smoke.is_some());
        let settings = runtime.view().settings;
        let curve = settings.curve();
        let selected = settings.profile;
        let dpi = unsafe { GetDpiForSystem() } as f64 / 96.;
        Self {
            runtime,
            settings,
            curve,
            selected,
            hwnd: HWND::default(),
            combo: HWND::default(),
            metrics: HWND::default(),
            status: HWND::default(),
            active: HWND::default(),
            description: HWND::default(),
            points: HWND::default(),
            apply: HWND::default(),
            calibrate: HWND::default(),
            startup: HWND::default(),
            footer: HWND::default(),
            font: HFONT::default(),
            metric_font: HFONT::default(),
            icon: HICON::default(),
            dpi,
            loading: false,
            taskbar_message: unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) },
            last_tooltip: String::new(),
            smoke: smoke.map(|path| Smoke {
                path,
                started: Instant::now(),
                stage: 0,
                initial_hidden: false,
                minimize_hides: false,
                presets_valid: false,
                manual_controls_absent: false,
                capture_error: None,
            }),
            tray_added: false,
            heat_alert_shown: false,
        }
    }
    fn px(&self, v: i32) -> i32 {
        (v as f64 * self.dpi).round() as i32
    }
    #[allow(clippy::too_many_arguments)] // Mirrors the native control's label, ID, geometry and style.
    unsafe fn child(
        &self,
        class: PCWSTR,
        text: &str,
        id: usize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        style: u32,
    ) -> HWND {
        unsafe {
            let text = wide(text);
            let child = CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class,
                PCWSTR(text.as_ptr()),
                WS_CHILD | WS_VISIBLE | WINDOW_STYLE(style),
                self.px(x),
                self.px(y),
                self.px(width),
                self.px(height),
                Some(self.hwnd),
                Some(HMENU(id as *mut _)),
                None,
                None,
            )
            .unwrap_or_default();
            SendMessageW(
                child,
                WM_SETFONT,
                Some(WPARAM(self.font.0 as usize)),
                Some(LPARAM(1)),
            );
            child
        }
    }
    unsafe fn initialize(&mut self) -> Result<(), String> {
        unsafe {
            self.font = CreateFontW(
                -self.px(14),
                0,
                0,
                0,
                400,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
                w!("Segoe UI"),
            );
            self.metric_font = CreateFontW(
                -self.px(25),
                0,
                0,
                0,
                600,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                CLEARTYPE_QUALITY,
                DEFAULT_PITCH.0 as u32 | FF_DONTCARE.0 as u32,
                w!("Segoe UI"),
            );
            self.child(w!("STATIC"), "A-Fan", 1, 24, 18, 150, 30, 0);
            self.child(
                w!("STATIC"),
                "Aspire A515-45  ·  Controle térmico",
                2,
                24,
                48,
                710,
                24,
                0,
            );
            self.metrics = self.child(
                w!("STATIC"),
                "Conectando aos sensores…",
                3,
                24,
                86,
                710,
                42,
                0,
            );
            SendMessageW(
                self.metrics,
                WM_SETFONT,
                Some(WPARAM(self.metric_font.0 as usize)),
                Some(LPARAM(1)),
            );
            self.status = self.child(w!("STATIC"), "", 4, 24, 135, 710, 56, 0);
            self.child(w!("STATIC"), "Preset de ventilação", 5, 24, 204, 210, 20, 0);
            self.combo = self.child(
                w!("COMBOBOX"),
                "",
                ID_PROFILE,
                24,
                229,
                240,
                200,
                WS_TABSTOP.0 | CBS_DROPDOWNLIST as u32 | WS_VSCROLL.0,
            );
            for profile in Profile::ALL {
                let name = wide(profile.label());
                SendMessageW(
                    self.combo,
                    CB_ADDSTRING,
                    Some(WPARAM(0)),
                    Some(LPARAM(name.as_ptr() as isize)),
                );
            }
            self.apply = self.child(
                w!("BUTTON"),
                "Aplicar perfil",
                ID_APPLY,
                280,
                228,
                142,
                30,
                WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
            );
            self.calibrate = self.child(
                w!("BUTTON"),
                "Calibrar controle…",
                ID_CALIBRATE,
                436,
                228,
                176,
                30,
                WS_TABSTOP.0 | BS_PUSHBUTTON as u32,
            );
            self.active = self.child(
                w!("STATIC"),
                "Ativo: Automático Acer",
                6,
                24,
                269,
                710,
                24,
                0,
            );
            self.child(w!("STATIC"), "Prévia do preset", 7, 24, 308, 360, 24, 0);
            self.child(
                w!("STATIC"),
                "Comportamento do preset",
                8,
                446,
                308,
                265,
                24,
                0,
            );
            self.description = self.child(w!("STATIC"), "", 11, 446, 341, 265, 66, 0);
            self.points = self.child(w!("STATIC"), "", 12, 446, 416, 265, 138, 0);
            self.child(
                w!("STATIC"),
                "Presets A-Fan: mínimo de 40% ou o mínimo calibrado, se maior.
100% até 80°C; alerta a 85°C. Os limites não garantem a temperatura da CPU.",
                9,
                24,
                577,
                690,
                46,
                0,
            );
            self.startup = self.child(
                w!("BUTTON"),
                "Iniciar com o Windows",
                ID_STARTUP,
                24,
                650,
                225,
                25,
                WS_TABSTOP.0 | BS_AUTOCHECKBOX as u32,
            );
            SendMessageW(
                self.startup,
                BM_SETCHECK,
                Some(WPARAM(if platform::startup_enabled() {
                    BST_CHECKED.0 as usize
                } else {
                    BST_UNCHECKED.0 as usize
                })),
                Some(LPARAM(0)),
            );
            let _ = EnableWindow(self.startup, platform::installed());
            self.child(
                w!("BUTTON"),
                "Salvar diagnóstico",
                ID_DIAGNOSTIC,
                266,
                647,
                160,
                30,
                WS_TABSTOP.0,
            );
            self.child(
                w!("BUTTON"),
                "Abrir pasta",
                ID_FOLDER,
                440,
                647,
                120,
                30,
                WS_TABSTOP.0,
            );
            self.child(
                w!("BUTTON"),
                "Sair",
                ID_EXIT,
                590,
                647,
                118,
                30,
                WS_TABSTOP.0,
            );
            self.footer = self.child(
                w!("STATIC"),
                "Fechar a janela mantém o A-Fan na bandeja.",
                10,
                24,
                693,
                690,
                23,
                0,
            );
            self.icon = LoadIconW(
                Some(GetModuleHandleW(None).map_err(|e| e.to_string())?.into()),
                PCWSTR(std::ptr::without_provenance(1)),
            )
            .or_else(|_| LoadIconW(None, IDI_APPLICATION))
            .map_err(|e| e.to_string())?;
            SendMessageW(
                self.hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_BIG as usize)),
                Some(LPARAM(self.icon.0 as isize)),
            );
            SendMessageW(
                self.hwnd,
                WM_SETICON,
                Some(WPARAM(ICON_SMALL as usize)),
                Some(LPARAM(self.icon.0 as isize)),
            );
            self.load_curve();
            self.add_tray()?;
            SetTimer(Some(self.hwnd), 1, 500, None);
            self.refresh();
            Ok(())
        }
    }
    unsafe fn tray_data(&self) -> NOTIFYICONDATAW {
        NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: self.hwnd,
            uID: 1,
            uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP,
            uCallbackMessage: TRAY_MESSAGE,
            hIcon: self.icon,
            ..Default::default()
        }
    }
    unsafe fn add_tray(&mut self) -> Result<(), String> {
        unsafe {
            let mut data = self.tray_data();
            write_wide(&mut data.szTip, "A-Fan · Automático Acer");
            if !Shell_NotifyIconW(NIM_ADD, &data).as_bool() {
                return Err("O Windows não confirmou o ícone na bandeja.".into());
            }
            self.tray_added = true;
            data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            let _ = Shell_NotifyIconW(NIM_SETVERSION, &data);
            Ok(())
        }
    }
    unsafe fn load_curve(&mut self) {
        unsafe {
            self.loading = true;
            SendMessageW(
                self.combo,
                CB_SETCURSEL,
                Some(WPARAM(
                    Profile::ALL
                        .iter()
                        .position(|p| *p == self.selected)
                        .unwrap(),
                )),
                Some(LPARAM(0)),
            );
            set_text(self.description, self.selected.description());
            let points = if self.selected == Profile::Firmware {
                "O A-Fan não define limites nem uma curva para o automático Acer.".into()
            } else {
                self.curve
                    .points
                    .iter()
                    .map(|p| format!("{}°C   →   {}%", format_number(p.celsius), p.percent))
                    .collect::<Vec<_>>()
                    .join("\r\n")
            };
            set_text(self.points, &points);
            self.loading = false;
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }
    unsafe fn refresh(&mut self) {
        unsafe {
            let view = self.runtime.view();
            let fresh = view.last_sample.is_some_and(|t| t.elapsed() <= STALE_AFTER);
            let temp = if fresh {
                view.telemetry
                    .temperature
                    .map(|t| format!("{t:.0}°C"))
                    .unwrap_or_else(|| "—".into())
            } else {
                "—".into()
            };
            let rpm = if fresh {
                view.telemetry
                    .rpm
                    .map(|r| format!("{r} RPM"))
                    .unwrap_or_else(|| "—".into())
            } else {
                "—".into()
            };
            set_text(
                self.metrics,
                &format!("CPU  {temp}     ·     Ventoinha  {rpm}"),
            );
            let hot = fresh && view.telemetry.temperature.is_some_and(|t| t >= MAX_TEMP);
            set_text(
                self.status,
                if hot {
                    "Temperatura elevada (85°C ou mais). Reduza a carga e verifique a ventilação do notebook."
                } else {
                    &view.message
                },
            );
            if hot && !self.heat_alert_shown {
                let mut data = self.tray_data();
                data.uFlags = NIF_INFO;
                data.dwInfoFlags = NIIF_WARNING;
                write_wide(&mut data.szInfoTitle, "A-Fan · Temperatura elevada");
                write_wide(
                    &mut data.szInfo,
                    "CPU a 85°C ou mais. Verifique a ventilação e reduza a carga.",
                );
                let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
            }
            if hot {
                self.heat_alert_shown = true;
            } else if fresh && view.telemetry.temperature.is_some_and(|t| t < 80.) {
                self.heat_alert_shown = false;
            }
            set_text(
                self.active,
                &format!(
                    "Ativo: {}{}",
                    if view.recovery_failed {
                        "Indeterminado · retorno à Acer não confirmado"
                    } else {
                        view.active.label()
                    },
                    if view.calibrating {
                        " · Calibração em andamento"
                    } else {
                        ""
                    }
                ),
            );
            let available = view.telemetry.controls_available
                && view.calibrated
                && fresh
                && !view.calibrating
                && self.smoke.is_none();
            let _ = EnableWindow(self.apply, self.selected == Profile::Firmware || available);
            let _ = EnableWindow(
                self.calibrate,
                view.telemetry.controls_available
                    && fresh
                    && !view.calibrating
                    && self.smoke.is_none(),
            );
            let tooltip = format!(
                "A-Fan · {temp} · {rpm}\n{}",
                if view.recovery_failed {
                    "Retorno à Acer não confirmado"
                } else {
                    view.active.label()
                }
            );
            if tooltip != self.last_tooltip {
                let mut data = self.tray_data();
                data.uFlags = NIF_TIP | NIF_SHOWTIP;
                write_wide(&mut data.szTip, &tooltip);
                let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
                self.last_tooltip = tooltip;
            }
            if IsWindowVisible(self.hwnd).as_bool() {
                let graph = self.graph_rect();
                let _ = InvalidateRect(Some(self.hwnd), Some(&graph), false);
            }
            if view.calibrated && self.settings.calibration != view.settings.calibration {
                self.settings = view.settings.clone();
                self.curve = self.settings.curve();
                self.selected = self.settings.profile;
                self.load_curve();
            }
            if view.exit_ready {
                let _ = PostMessageW(Some(self.hwnd), EXIT_MESSAGE, WPARAM(0), LPARAM(0));
                return;
            }
            self.smoke_step(&view);
        }
    }
    unsafe fn smoke_step(&mut self, view: &crate::runtime::View) {
        unsafe {
            let Some(mut probe) = self.smoke.take() else {
                return;
            };
            let delay = if probe.stage == 0 { 5 } else { 1 };
            if probe.started.elapsed() < Duration::from_secs(delay) {
                self.smoke = Some(probe);
                return;
            }
            match probe.stage {
                0 => {
                    probe.initial_hidden = !IsWindowVisible(self.hwnd).as_bool();
                    let original = self.selected;
                    probe.presets_valid = true;
                    for profile in Profile::ALL {
                        self.selected = profile;
                        self.curve = Curve::preset(profile, self.settings.floor());
                        self.load_curve();
                        let index = SendMessageW(
                            self.combo,
                            CB_GETCURSEL,
                            Some(WPARAM(0)),
                            Some(LPARAM(0)),
                        )
                        .0;
                        probe.presets_valid &= index >= 0
                            && Profile::ALL.get(index as usize) == Some(&profile)
                            && self.curve.validate(self.settings.floor()).is_ok()
                            && get_text(self.combo) == profile.label()
                            && get_text(self.description) == profile.description();
                    }
                    probe.manual_controls_absent =
                        FindWindowExW(Some(self.hwnd), None, w!("EDIT"), PCWSTR::null()).is_err()
                            && [103, 104, 105, 106]
                                .iter()
                                .all(|id| GetDlgItem(Some(self.hwnd), *id).is_err());
                    self.selected = original;
                    self.curve = Curve::preset(original, self.settings.floor());
                    self.load_curve();
                    if !view.telemetry.controls_available {
                        self.runtime.send(Request::Apply(self.settings.clone()));
                    }
                    let _ = ShowWindow(self.hwnd, SW_SHOW);
                    probe.capture_error = self
                        .snapshot(&Path::new(&probe.path).with_extension("bmp"))
                        .err();
                    let _ = PostMessageW(
                        Some(self.hwnd),
                        WM_SYSCOMMAND,
                        WPARAM(SC_MINIMIZE as usize),
                        LPARAM(0),
                    );
                    probe.stage = 1;
                    probe.started = Instant::now();
                    self.smoke = Some(probe);
                }
                1 => {
                    probe.minimize_hides = !IsWindowVisible(self.hwnd).as_bool();
                    let _ = ShowWindow(self.hwnd, SW_SHOW);
                    let _ = PostMessageW(Some(self.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                    probe.stage = 2;
                    probe.started = Instant::now();
                    self.smoke = Some(probe);
                }
                _ => {
                    let report = serde_json::json!({"window_created":true,"starts_hidden":probe.initial_hidden,"minimize_hides":probe.minimize_hides,
                    "close_hides":!IsWindowVisible(self.hwnd).as_bool(),"tray_added":self.tray_added,"presets_validated":probe.presets_valid,"manual_controls_absent":probe.manual_controls_absent,"preset_count":Profile::ALL.len()-1,
                    "capture_error":probe.capture_error,"temperature":view.telemetry.temperature,"rpm":view.telemetry.rpm,
                    "controls_available":view.telemetry.controls_available,"apply_enabled":IsWindowEnabled(self.apply).as_bool(),
                    "calibrate_enabled":IsWindowEnabled(self.calibrate).as_bool(),"active_profile":view.active,
                    "selected_profile_index":SendMessageW(self.combo,CB_GETCURSEL,Some(WPARAM(0)),Some(LPARAM(0))).0,
                    "hardware_writes":crate::hardware::write_count(),"message":view.message});
                    let _ = storage::write_json(Path::new(&probe.path), &report);
                    self.runtime.send(Request::Exit);
                }
            }
        }
    }
    unsafe fn action(&mut self, id: usize) -> Result<(), String> {
        unsafe {
            match id {
                ID_OPEN => {
                    let _ = ShowWindow(self.hwnd, SW_SHOW);
                    let _ = SetForegroundWindow(self.hwnd);
                }
                ID_APPLY => {
                    self.settings.profile = self.selected;
                    self.runtime.send(Request::Apply(self.settings.clone()));
                }
                ID_CALIBRATE => {
                    if MessageBoxW(
                        Some(self.hwnd),
                        w!(
                            "A calibração aumentará primeiro a ventoinha e depois testará reduções graduais. Leva até 3 minutos. Mantenha o notebook aberto, sem carga pesada. Continuar?"
                        ),
                        w!("Calibração A-Fan"),
                        MB_YESNO | MB_ICONINFORMATION,
                    ) == IDYES
                    {
                        self.runtime.send(Request::Calibrate);
                    }
                }
                ID_STARTUP => {
                    let enabled =
                        SendMessageW(self.startup, BM_GETCHECK, Some(WPARAM(0)), Some(LPARAM(0))).0
                            == BST_CHECKED.0 as isize;
                    if let Err(e) = platform::set_startup(enabled) {
                        SendMessageW(
                            self.startup,
                            BM_SETCHECK,
                            Some(WPARAM((!enabled) as usize)),
                            Some(LPARAM(0)),
                        );
                        return Err(e);
                    }
                    set_text(
                        self.footer,
                        if enabled {
                            "Inicialização com o Windows ativada."
                        } else {
                            "Inicialização com o Windows desativada."
                        },
                    );
                }
                ID_DIAGNOSTIC => {
                    self.runtime.send(Request::Diagnostic);
                    set_text(
                        self.footer,
                        "Salvando diagnóstico local, sem alterar a ventoinha…",
                    );
                }
                ID_FOLDER => platform::reveal_data_folder(),
                ID_FIRMWARE => self.runtime.send(Request::Firmware),
                ID_EXIT => {
                    self.runtime.send(Request::Exit);
                    set_text(
                        self.footer,
                        "Confirmando retorno ao automático Acer antes de sair…",
                    );
                }
                value
                    if (ID_MENU_PROFILE..ID_MENU_PROFILE + Profile::ALL.len()).contains(&value) =>
                {
                    self.selected = Profile::ALL[value - ID_MENU_PROFILE];
                    self.settings.profile = self.selected;
                    self.curve = self.settings.curve();
                    self.load_curve();
                    self.runtime.send(Request::Apply(self.settings.clone()));
                }
                _ => {}
            }
            Ok(())
        }
    }
    unsafe fn menu(&mut self) {
        unsafe {
            let Ok(menu) = CreatePopupMenu() else {
                return;
            };
            let _ = AppendMenuW(menu, MF_STRING, ID_OPEN, w!("Abrir A-Fan"));
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            let view = self.runtime.view();
            for (index, profile) in Profile::ALL.iter().enumerate() {
                let mut flags = MF_STRING;
                if *profile == view.active && !view.recovery_failed {
                    flags |= MF_CHECKED;
                }
                if *profile != Profile::Firmware
                    && (!view.calibrated || !view.telemetry.controls_available || view.calibrating)
                {
                    flags |= MF_GRAYED;
                }
                let name = wide(profile.label());
                let _ = AppendMenuW(menu, flags, ID_MENU_PROFILE + index, PCWSTR(name.as_ptr()));
            }
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(menu, MF_STRING, ID_EXIT, w!("Sair (restaurar Acer)"));
            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            let _ = SetForegroundWindow(self.hwnd);
            let command = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                None,
                self.hwnd,
                None,
            )
            .0;
            if command > 0
                && let Err(e) = self.action(command as usize)
            {
                platform::error(&e);
            }
            let _ = DestroyMenu(menu);
            let _ = PostMessageW(Some(self.hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        }
    }
    fn graph_rect(&self) -> RECT {
        RECT {
            left: self.px(24),
            top: self.px(340),
            right: self.px(412),
            bottom: self.px(554),
        }
    }
    unsafe fn paint(&self) {
        unsafe {
            let mut paint = PAINTSTRUCT::default();
            let dc = BeginPaint(self.hwnd, &mut paint);
            self.draw_graph(dc);
            let _ = EndPaint(self.hwnd, &paint);
        }
    }
    unsafe fn draw_graph(&self, dc: HDC) {
        unsafe {
            let graph = self.graph_rect();
            let _ = FillRect(dc, &graph, GetSysColorBrush(COLOR_WINDOW));
            let old_font = SelectObject(dc, self.font.into());
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x484848));
            if self.selected == Profile::Firmware {
                let text: Vec<u16> = "Curva administrada pelo firmware Acer"
                    .encode_utf16()
                    .collect();
                let _ = TextOutW(dc, self.px(30), self.px(400), &text);
                SelectObject(dc, old_font);
                return;
            }
            let left = self.px(65);
            let top = self.px(355);
            let width = self.px(326);
            let height = self.px(164);
            let map = |temp: f64, percent: u8| POINT {
                x: left + ((temp - 20.) / 65. * width as f64) as i32,
                y: top + height - ((percent as f64) / 100. * height as f64) as i32,
            };
            let grid = CreatePen(PS_SOLID, 1, COLORREF(0xE2E2E2));
            let old_pen = SelectObject(dc, grid.into());
            for percent in [0, 25, 50, 75, 100] {
                let y = map(20., percent).y;
                let _ = MoveToEx(dc, left, y, None);
                let _ = LineTo(dc, left + width, y);
                let label: Vec<u16> = format!("{percent}%").encode_utf16().collect();
                let _ = TextOutW(dc, self.px(25), y - self.px(8), &label);
            }
            for temperature in [20., 40., 60., 85.] {
                let x = map(temperature, 0).x;
                let _ = MoveToEx(dc, x, top, None);
                let _ = LineTo(dc, x, top + height);
                let label: Vec<u16> = format!("{temperature:.0}°").encode_utf16().collect();
                let _ = TextOutW(dc, x - self.px(10), top + height + self.px(8), &label);
            }
            let line = CreatePen(PS_SOLID, self.px(3).max(1), COLORREF(0x8A6910));
            SelectObject(dc, line.into());
            let mut points = vec![map(20., self.curve.percent(20.))];
            points.extend(self.curve.points.iter().map(|p| map(p.celsius, p.percent)));
            let _ = Polyline(dc, &points);
            let brush = CreateSolidBrush(COLORREF(0x8A6910));
            let old_brush = SelectObject(dc, brush.into());
            for p in &self.curve.points {
                let point = map(p.celsius, p.percent);
                let _ = Ellipse(
                    dc,
                    point.x - self.px(4),
                    point.y - self.px(4),
                    point.x + self.px(4),
                    point.y + self.px(4),
                );
            }
            if let Some(temp) = self.runtime.view().telemetry.temperature
                && (20.0..=85.0).contains(&temp)
            {
                let marker = CreatePen(PS_DOT, 1, COLORREF(0x505050));
                SelectObject(dc, marker.into());
                let x = map(temp, 0).x;
                let _ = MoveToEx(dc, x, top, None);
                let _ = LineTo(dc, x, top + height);
                SelectObject(dc, line.into());
                let _ = DeleteObject(marker.into());
            }
            SelectObject(dc, old_brush);
            SelectObject(dc, old_pen);
            SelectObject(dc, old_font);
            let _ = DeleteObject(brush.into());
            let _ = DeleteObject(line.into());
            let _ = DeleteObject(grid.into());
        }
    }
    unsafe fn snapshot(&self, path: &Path) -> Result<(), String> {
        unsafe {
            let mut rect = RECT::default();
            GetClientRect(self.hwnd, &mut rect).map_err(|e| e.to_string())?;
            let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
            if width <= 0 || height <= 0 || width > 4096 || height > 4096 {
                return Err("Dimensões de captura inválidas.".into());
            }
            let screen = GetDC(Some(self.hwnd));
            let dc = CreateCompatibleDC(Some(screen));
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: 40,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = std::ptr::null_mut();
            let bitmap =
                match CreateDIBSection(Some(screen), &info, DIB_RGB_COLORS, &mut pixels, None, 0) {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = DeleteDC(dc);
                        ReleaseDC(Some(self.hwnd), screen);
                        return Err(e.to_string());
                    }
                };
            let old = SelectObject(dc, bitmap.into());
            let mut window = RECT::default();
            GetWindowRect(self.hwnd, &mut window).map_err(|e| e.to_string())?;
            let mut origin = POINT::default();
            let _ = ClientToScreen(self.hwnd, &mut origin);
            let _ = SetViewportOrgEx(dc, window.left - origin.x, window.top - origin.y, None);
            let _ = SendMessageW(
                self.hwnd,
                WM_PRINT,
                Some(WPARAM(dc.0 as usize)),
                Some(LPARAM(
                    (PRF_CLIENT | PRF_CHILDREN | PRF_ERASEBKGND) as isize,
                )),
            );
            let _ = SetViewportOrgEx(dc, 0, 0, None);
            self.draw_graph(dc);
            let length = (width * height * 4) as usize;
            let mut bytes = Vec::with_capacity(length + 54);
            bytes.extend_from_slice(b"BM");
            bytes.extend_from_slice(&((length + 54) as u32).to_le_bytes());
            bytes.extend_from_slice(&[0; 4]);
            bytes.extend_from_slice(&54u32.to_le_bytes());
            bytes.extend_from_slice(&40u32.to_le_bytes());
            bytes.extend_from_slice(&width.to_le_bytes());
            bytes.extend_from_slice(&(-height).to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.extend_from_slice(&32u16.to_le_bytes());
            bytes.extend_from_slice(&[0; 24]);
            bytes.extend_from_slice(std::slice::from_raw_parts(pixels.cast::<u8>(), length));
            SelectObject(dc, old);
            let _ = DeleteObject(bitmap.into());
            let _ = DeleteDC(dc);
            ReleaseDC(Some(self.hwnd), screen);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            std::fs::write(path, bytes).map_err(|e| e.to_string())
        }
    }
}
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
            (*(create.lpCreateParams as *const RefCell<App>))
                .borrow_mut()
                .hwnd = hwnd;
        }
        let pointer = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const RefCell<App>;
        if pointer.is_null() {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        if message == WM_CTLCOLORSTATIC {
            let dc = HDC(wparam.0 as *mut _);
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x202020));
            return LRESULT(GetSysColorBrush(COLOR_WINDOW).0 as isize);
        }
        if message == WM_CLOSE || (message == WM_SIZE && wparam.0 == SIZE_MINIMIZED as usize) {
            let _ = ShowWindow(hwnd, SW_HIDE);
            return LRESULT(0);
        }
        if message == EXIT_MESSAGE {
            let _ = DestroyWindow(hwnd);
            return LRESULT(0);
        }
        if message == WM_NCDESTROY {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            return DefWindowProcW(hwnd, message, wparam, lparam);
        }
        let Ok(mut app) = (*pointer).try_borrow_mut() else {
            return DefWindowProcW(hwnd, message, wparam, lparam);
        };
        if message == app.taskbar_message && app.taskbar_message != 0 {
            app.tray_added = false;
            let _ = app.add_tray();
            return LRESULT(0);
        }
        match message {
            WM_CLOSE => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_SIZE if wparam.0 == SIZE_MINIMIZED as usize => {
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_COMMAND => {
                let id = wparam.0 & 0xffff;
                let notification = (wparam.0 >> 16) & 0xffff;
                if id == ID_PROFILE && notification == CBN_SELCHANGE as usize && !app.loading {
                    let index =
                        SendMessageW(app.combo, CB_GETCURSEL, Some(WPARAM(0)), Some(LPARAM(0))).0;
                    if (0..Profile::ALL.len() as isize).contains(&index) {
                        app.selected = Profile::ALL[index as usize];
                        app.curve = Curve::preset(app.selected, app.settings.floor());
                        app.load_curve();
                        app.refresh();
                    }
                } else if notification == BN_CLICKED as usize
                    && let Err(error) = app.action(id)
                {
                    platform::error(&error);
                }
                LRESULT(0)
            }
            WM_TIMER => {
                app.refresh();
                LRESULT(0)
            }
            WM_PAINT => {
                app.paint();
                LRESULT(0)
            }
            WM_POWERBROADCAST => {
                match wparam.0 as u32 {
                    PBT_APMSUSPEND => app.runtime.send(Request::Suspend),
                    PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMESUSPEND => {
                        app.runtime.send(Request::Resume)
                    }
                    _ => {}
                }
                LRESULT(1)
            }
            WM_QUERYENDSESSION => {
                app.runtime.send(Request::Exit);
                LRESULT(1)
            }
            TRAY_MESSAGE => {
                match lparam.0 as u32 & 0xffff {
                    WM_LBUTTONUP | NIN_SELECT | TRAY_KEY_SELECT => {
                        let _ = app.action(ID_OPEN);
                    }
                    WM_RBUTTONUP | WM_CONTEXTMENU => app.menu(),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                KillTimer(Some(hwnd), 1).ok();
                if app.tray_added {
                    let _ = Shell_NotifyIconW(NIM_DELETE, &app.tray_data());
                }
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
unsafe fn set_text(hwnd: HWND, text: &str) {
    unsafe {
        if get_text(hwnd) != text {
            let value = wide(text);
            let _ = SetWindowTextW(hwnd, PCWSTR(value.as_ptr()));
        }
    }
}
unsafe fn get_text(hwnd: HWND) -> String {
    unsafe {
        let mut value = [0u16; 2048];
        let length = GetWindowTextW(hwnd, &mut value) as usize;
        String::from_utf16_lossy(&value[..length])
    }
}
fn write_wide<const N: usize>(target: &mut [u16; N], text: &str) {
    for (out, ch) in target.iter_mut().take(N - 1).zip(text.encode_utf16()) {
        *out = ch;
    }
}
fn format_number(number: f64) -> String {
    if number.fract() == 0. {
        format!("{number:.0}")
    } else {
        format!("{number:.1}")
    }
}

pub fn run(show: bool) -> Result<(), String> {
    run_inner(show, None)
}
pub fn smoke_test(path: &str) -> Result<(), String> {
    run_inner(false, Some(path.into()))
}
fn run_inner(show: bool, smoke: Option<String>) -> Result<(), String> {
    unsafe {
        if !platform::elevated() && platform::installed() && smoke.is_none() {
            return platform::launch_registered_task();
        }
        let mutex =
            CreateMutexW(None, false, w!("Local\\A-Fan-Controller")).map_err(|e| e.to_string())?;
        let already_running = GetLastError() == ERROR_ALREADY_EXISTS;
        let _mutex = OwnedHandle(mutex);
        if already_running {
            return Ok(());
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
        let class = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: w!("AFanWindow"),
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(|e| e.to_string())?,
            hbrBackground: GetSysColorBrush(COLOR_WINDOW),
            ..Default::default()
        };
        RegisterClassW(&class);
        let app = Box::new(RefCell::new(App::new(smoke)));
        let width = app.borrow().px(760);
        let height = app.borrow().px(770);
        let pointer = &*app as *const RefCell<App>;
        let hwnd = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            w!("AFanWindow"),
            w!("A-Fan"),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            Some(pointer.cast()),
        )
        .map_err(|e| e.to_string())?;
        let initialized = app.borrow_mut().initialize();
        if let Err(error) = initialized {
            let _ = DestroyWindow(hwnd);
            return Err(error);
        }
        if show {
            let _ = ShowWindow(hwnd, SW_SHOW);
        }
        let mut message = MSG::default();
        loop {
            let result = GetMessageW(&mut message, None, 0, 0).0;
            if result == 0 {
                break;
            }
            if result == -1 {
                return Err("A fila de mensagens do Windows falhou.".into());
            }
            if !IsDialogMessageW(hwnd, &message).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        let _ = DeleteObject(app.borrow().font.into());
        let _ = DeleteObject(app.borrow().metric_font.into());
        Ok(())
    }
}

pub fn request_exit() -> Result<(), String> {
    unsafe {
        if let Ok(hwnd) = FindWindowW(w!("AFanWindow"), PCWSTR::null()) {
            PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(ID_EXIT), LPARAM(0))
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tooltip_is_bounded_and_terminated() {
        let mut tip = [0u16; 5];
        write_wide(&mut tip, "ABCDEF");
        assert_eq!(tip, [65, 66, 67, 68, 0]);
    }
}
