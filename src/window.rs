use chrono::{DateTime, Utc};
use cosmic::app::Core;
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::{Color, Limits};
use cosmic::iced_runtime::core::window;
use cosmic::widget;
use cosmic::{Action, Element, Task};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::api;

pub const APP_ID: &str = "com.github.mherczeg.claude-usage-applet";
const POLL_SECONDS: u64 = 600;
const SYNC_SECONDS: u64 = 1;
const STATE_FILE_NAME: &str = "shared-state.json";
const STATE_LOCK_FILE_NAME: &str = "state.lock";
const FETCH_LOCK_FILE_NAME: &str = "fetch.lock";
const STATE_LOCK_STALE_SECONDS: u64 = 5;
const FETCH_LOCK_STALE_SECONDS: u64 = POLL_SECONDS * 2;
const STATE_LOCK_ATTEMPTS: usize = 40;
const LOCK_RETRY_DELAY_MILLIS: u64 = 50;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct SharedSnapshot {
    usage_data: Option<api::UsageData>,
    error: Option<String>,
    loading: bool,
    next_reset_at: Option<DateTime<Utc>>,
    paused: bool,
}

impl Default for SharedSnapshot {
    fn default() -> Self {
        Self {
            usage_data: None,
            error: None,
            loading: false,
            next_reset_at: None,
            paused: false,
        }
    }
}

#[derive(Clone, Copy)]
enum FetchReason {
    Initial,
    Poll,
    ResetCheck,
    Refresh,
}

enum FetchOutcome {
    Started(SharedSnapshot),
    Skipped(SharedSnapshot),
}

struct FileLock {
    path: PathBuf,
}

impl FileLock {
    fn acquire(
        file_name: &str,
        stale_after: Duration,
        attempts: usize,
    ) -> Result<Option<Self>, String> {
        let path = shared_file_path(file_name)?;

        for attempt in 0..attempts {
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(mut file) => {
                    file.write_all(std::process::id().to_string().as_bytes())
                        .map_err(|e| format!("write lock {}: {e}", path.display()))?;
                    return Ok(Some(Self { path }));
                }
                Err(e) if e.kind() == ErrorKind::AlreadyExists => {
                    if lock_is_stale(&path, stale_after)? {
                        match fs::remove_file(&path) {
                            Ok(()) => continue,
                            Err(err) if err.kind() == ErrorKind::NotFound => continue,
                            Err(err) => {
                                return Err(format!("remove stale lock {}: {err}", path.display()));
                            }
                        }
                    }

                    if attempt + 1 == attempts {
                        return Ok(None);
                    }

                    std::thread::sleep(Duration::from_millis(LOCK_RETRY_DELAY_MILLIS));
                }
                Err(e) => return Err(format!("create lock {}: {e}", path.display())),
            }
        }

        Ok(None)
    }

    fn release(self) -> Result<(), String> {
        release_lock_path(&self.path)
    }
}

fn shared_state_dir() -> Result<PathBuf, String> {
    let base_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base_dir.join(APP_ID);
    fs::create_dir_all(&dir)
        .map_err(|e| format!("create shared state dir {}: {e}", dir.display()))?;
    Ok(dir)
}

fn shared_file_path(file_name: &str) -> Result<PathBuf, String> {
    Ok(shared_state_dir()?.join(file_name))
}

fn lock_is_stale(path: &Path, stale_after: Duration) -> Result<bool, String> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(format!("read lock metadata {}: {e}", path.display())),
    };

    let modified = metadata
        .modified()
        .map_err(|e| format!("read lock timestamp {}: {e}", path.display()))?;
    modified
        .elapsed()
        .map(|elapsed| elapsed >= stale_after)
        .map_err(|e| format!("compute lock age {}: {e}", path.display()))
}

fn release_lock_path(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("remove lock {}: {e}", path.display())),
    }
}

fn read_shared_snapshot() -> Result<SharedSnapshot, String> {
    let path = shared_file_path(STATE_FILE_NAME)?;
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents)
            .map_err(|e| format!("parse shared state {}: {e}", path.display())),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(SharedSnapshot::default()),
        Err(e) => Err(format!("read shared state {}: {e}", path.display())),
    }
}

fn write_shared_snapshot(snapshot: &SharedSnapshot) -> Result<(), String> {
    let path = shared_file_path(STATE_FILE_NAME)?;
    let temp_path = path.with_extension(format!("tmp-{}", std::process::id()));
    let data = serde_json::to_vec(snapshot)
        .map_err(|e| format!("serialize shared state {}: {e}", path.display()))?;

    fs::write(&temp_path, data)
        .map_err(|e| format!("write shared state temp {}: {e}", temp_path.display()))?;
    fs::rename(&temp_path, &path)
        .map_err(|e| format!("replace shared state {}: {e}", path.display()))?;
    Ok(())
}

fn with_state_lock<T, F>(operation: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String>,
{
    let Some(lock) = FileLock::acquire(
        STATE_LOCK_FILE_NAME,
        Duration::from_secs(STATE_LOCK_STALE_SECONDS),
        STATE_LOCK_ATTEMPTS,
    )?
    else {
        return Err("timed out acquiring shared state lock".into());
    };

    let result = operation();
    let release_result = lock.release();

    match (result, release_result) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(release_error)) => Err(format!("{error}; {release_error}")),
    }
}

fn update_shared_snapshot<F>(mutate: F) -> Result<SharedSnapshot, String>
where
    F: FnOnce(&mut SharedSnapshot),
{
    with_state_lock(|| {
        let mut snapshot = read_shared_snapshot()?;
        mutate(&mut snapshot);
        write_shared_snapshot(&snapshot)?;
        Ok(snapshot)
    })
}

fn should_fetch(snapshot: &SharedSnapshot, reason: FetchReason) -> bool {
    if snapshot.paused {
        return false;
    }

    match reason {
        FetchReason::Initial => snapshot.usage_data.is_none() && !snapshot.loading,
        FetchReason::Poll | FetchReason::Refresh => true,
        FetchReason::ResetCheck => snapshot
            .next_reset_at
            .map(|reset_at| Utc::now() >= reset_at + chrono::Duration::minutes(1))
            .unwrap_or(false),
    }
}

fn begin_shared_fetch(reason: FetchReason) -> Result<FetchOutcome, String> {
    let Some(fetch_lock) = FileLock::acquire(
        FETCH_LOCK_FILE_NAME,
        Duration::from_secs(FETCH_LOCK_STALE_SECONDS),
        1,
    )?
    else {
        return Ok(FetchOutcome::Skipped(read_shared_snapshot()?));
    };

    let snapshot_result = with_state_lock(|| {
        let mut snapshot = read_shared_snapshot()?;
        let should_start = should_fetch(&snapshot, reason);

        if should_start {
            snapshot.loading = true;
            if matches!(reason, FetchReason::ResetCheck) {
                snapshot.next_reset_at = None;
            }
            write_shared_snapshot(&snapshot)?;
        }

        Ok((snapshot, should_start))
    });

    match snapshot_result {
        Ok((snapshot, true)) => Ok(FetchOutcome::Started(snapshot)),
        Ok((snapshot, false)) => {
            fetch_lock.release()?;
            Ok(FetchOutcome::Skipped(snapshot))
        }
        Err(error) => {
            let release_result = fetch_lock.release();
            match release_result {
                Ok(()) => Err(error),
                Err(release_error) => Err(format!("{error}; {release_error}")),
            }
        }
    }
}

fn finish_shared_fetch(result: Result<api::UsageData, String>) -> Result<SharedSnapshot, String> {
    let snapshot_result = with_state_lock(|| {
        let mut snapshot = read_shared_snapshot()?;
        snapshot.loading = false;

        match result {
            Ok(data) => {
                snapshot.next_reset_at = calculate_next_reset_at(&data);
                snapshot.usage_data = Some(data);
                snapshot.error = None;
            }
            Err(error) => {
                snapshot.error = Some(error);
            }
        }

        write_shared_snapshot(&snapshot)?;
        Ok(snapshot)
    });

    let release_result =
        shared_file_path(FETCH_LOCK_FILE_NAME).and_then(|path| release_lock_path(&path));

    match (snapshot_result, release_result) {
        (Ok(snapshot), Ok(())) => Ok(snapshot),
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(release_error)) => Err(format!("{error}; {release_error}")),
    }
}

fn calculate_next_reset_at(data: &api::UsageData) -> Option<DateTime<Utc>> {
    [
        data.five_hour.as_ref(),
        data.seven_day.as_ref(),
        data.seven_day_sonnet.as_ref(),
        data.seven_day_opus.as_ref(),
    ]
    .iter()
    .filter_map(|l| l.and_then(|l| l.resets_at.as_deref()))
    .filter_map(|s| DateTime::parse_from_rfc3339(s).ok())
    .map(|dt| dt.with_timezone(&Utc))
    .filter(|&dt| dt > Utc::now())
    .min()
}

// ── Model ───────────────────────────────────────────────────────────────

pub struct Window {
    core: Core,
    popup: Option<Id>,
    usage_data: Option<api::UsageData>,
    error: Option<String>,
    loading: bool,
    next_reset_at: Option<DateTime<Utc>>,
    paused: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            core: Core::default(),
            popup: None,
            usage_data: None,
            error: None,
            loading: true,
            next_reset_at: None,
            paused: false,
        }
    }
}

// ── Messages ────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    SyncShared,
    Tick,
    CheckReset,
    Refresh,
    TogglePause,
    SharedUsageLoaded(Result<SharedSnapshot, String>),
}

// ── Application impl ────────────────────────────────────────────────────

impl cosmic::Application for Window {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, _flags: Self::Flags) -> (Self, Task<Action<Self::Message>>) {
        let mut window = Window {
            core,
            loading: true,
            ..Default::default()
        };
        window.sync_from_shared();

        let task = window.trigger_shared_fetch(FetchReason::Initial);

        (window, task)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn subscription(&self) -> cosmic::iced::Subscription<Self::Message> {
        let mut subscriptions = vec![cosmic::iced::time::every(Duration::from_secs(SYNC_SECONDS))
            .map(|_| Message::SyncShared)];

        if !self.paused {
            subscriptions.push(
                cosmic::iced::time::every(Duration::from_secs(POLL_SECONDS)).map(|_| Message::Tick),
            );
            subscriptions.push(
                cosmic::iced::time::every(Duration::from_secs(60)).map(|_| Message::CheckReset),
            );
        }

        cosmic::iced::Subscription::batch(subscriptions)
    }

    fn update(&mut self, message: Self::Message) -> Task<Action<Self::Message>> {
        match message {
            Message::TogglePopup => {
                return if let Some(popup_id) = self.popup.take() {
                    destroy_popup(popup_id)
                } else {
                    let new_id = Id::unique();
                    self.popup.replace(new_id);

                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );

                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(100.0)
                        .max_height(600.0);

                    get_popup(popup_settings)
                }
            }
            Message::PopupClosed(popup_id) => {
                if self.popup.as_ref() == Some(&popup_id) {
                    self.popup = None;
                }
            }
            Message::SyncShared => {
                self.sync_from_shared();
            }
            Message::CheckReset => {
                return self.trigger_shared_fetch(FetchReason::ResetCheck);
            }
            Message::Tick => {
                return self.trigger_shared_fetch(FetchReason::Poll);
            }
            Message::Refresh => {
                return self.trigger_shared_fetch(FetchReason::Refresh);
            }
            Message::TogglePause => match update_shared_snapshot(|snapshot| {
                snapshot.paused = !snapshot.paused;
            }) {
                Ok(snapshot) => self.apply_shared_snapshot(snapshot),
                Err(error) => self.error = Some(error),
            },
            Message::SharedUsageLoaded(result) => match result {
                Ok(snapshot) => self.apply_shared_snapshot(snapshot),
                Err(error) => {
                    self.sync_from_shared();
                    self.loading = false;
                    self.error = Some(error);
                }
            },
        }
        Task::none()
    }

    fn view(&self) -> Element<'_, Self::Message> {
        let (label, bg_color) = if self.paused {
            let pct_label = self
                .usage_data
                .as_ref()
                .and_then(|d| d.five_hour.as_ref())
                .map(|l| format!("{:.0}%", l.utilization))
                .unwrap_or_else(|| "…%".to_string());
            (pct_label, Color::from_rgb(0.25, 0.35, 0.60))
        } else {
            match &self.usage_data {
                Some(data) => {
                    let pct = data
                        .five_hour
                        .as_ref()
                        .map(|l| l.utilization)
                        .unwrap_or(0.0);
                    let color = usage_color(pct);
                    (format!("{:.0}%", pct), color)
                }
                None => ("…%".to_string(), Color::from_rgba(0.5, 0.5, 0.5, 0.4)),
            }
        };

        let text = widget::text::caption_heading(label);

        widget::button::custom(widget::container(text).padding([1, 6]).class(
            cosmic::theme::Container::custom(move |_theme| cosmic::widget::container::Style {
                background: Some(bg_color.into()),
                border: cosmic::iced::Border {
                    radius: 6.0.into(),
                    ..Default::default()
                },
                text_color: Some(Color::WHITE),
                ..Default::default()
            }),
        ))
        .on_press(Message::TogglePopup)
        .class(cosmic::theme::Button::AppletIcon)
        .into()
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut content = widget::list_column().padding(10).spacing(4);

        if let Some(err) = &self.error {
            content = content.add(widget::settings::item(
                "Error",
                widget::text::body(err.clone()),
            ));
        }

        if let Some(data) = &self.usage_data {
            let limits: &[(&str, &Option<api::UsageLimit>)] = &[
                ("5-hour window", &data.five_hour),
                ("7-day total", &data.seven_day),
                ("7-day Sonnet", &data.seven_day_sonnet),
                ("7-day Opus", &data.seven_day_opus),
            ];

            let has_any_limit = limits.iter().any(|(_, l)| l.is_some());

            for (name, limit) in limits {
                if let Some(limit) = limit {
                    let pct = limit.utilization;
                    let resets = limit
                        .resets_at
                        .as_deref()
                        .map(api::format_reset_time)
                        .unwrap_or_else(|| "—".to_string());
                    let color = usage_color(pct);

                    content = content.add(
                        widget::column()
                            .spacing(4)
                            .push(
                                widget::row()
                                    .push(widget::text::heading(*name))
                                    .push(widget::horizontal_space())
                                    .push(widget::text::heading(format!("{pct:.1}%"))),
                            )
                            .push(widget::progress_bar(0.0..=100.0, pct as f32).class(
                                cosmic::theme::ProgressBar::custom(move |theme| {
                                    let cosmic = theme.cosmic();
                                    widget::progress_bar::Style {
                                        background: Color::from(cosmic.background.divider).into(),
                                        bar: color.into(),
                                        border: cosmic::iced::Border {
                                            radius: cosmic.corner_radii.radius_xl.into(),
                                            ..Default::default()
                                        },
                                    }
                                }),
                            ))
                            .push(widget::text::caption(format!("Resets in {resets}"))),
                    );
                }
            }
            if !has_any_limit {
                content = content.add(widget::settings::item(
                    "Status",
                    widget::text::body("No active usage"),
                ));
            }
        } else if self.loading {
            content = content.add(widget::settings::item(
                "Status",
                widget::text::body("Loading…"),
            ));
        }

        let pause_label = if self.paused {
            "▶ Resume polling"
        } else {
            "⏸ Pause polling"
        };
        content = content.add(widget::button::standard(pause_label).on_press(Message::TogglePause));

        let refresh_label = if self.loading {
            "Refreshing…"
        } else {
            "↻ Refresh"
        };
        let refresh_btn = widget::button::standard(refresh_label);
        content = content.add(if self.paused {
            refresh_btn
        } else {
            refresh_btn.on_press(Message::Refresh)
        });

        self.core.applet.popup_container(content).into()
    }
}

impl Window {
    fn apply_shared_snapshot(&mut self, snapshot: SharedSnapshot) {
        self.usage_data = snapshot.usage_data;
        self.error = snapshot.error;
        self.loading = snapshot.loading;
        self.next_reset_at = snapshot.next_reset_at;
        self.paused = snapshot.paused;
    }

    fn sync_from_shared(&mut self) {
        match read_shared_snapshot() {
            Ok(snapshot) => self.apply_shared_snapshot(snapshot),
            Err(error) => {
                self.loading = false;
                self.error = Some(error);
            }
        }
    }

    fn trigger_shared_fetch(&mut self, reason: FetchReason) -> Task<Action<Message>> {
        match begin_shared_fetch(reason) {
            Ok(FetchOutcome::Started(snapshot)) => {
                self.apply_shared_snapshot(snapshot);
                Task::future(async {
                    let result = api::fetch_usage().await;
                    Message::SharedUsageLoaded(finish_shared_fetch(result))
                })
                .map(cosmic::Action::from)
            }
            Ok(FetchOutcome::Skipped(snapshot)) => {
                self.apply_shared_snapshot(snapshot);
                Task::none()
            }
            Err(error) => {
                self.loading = false;
                self.error = Some(error);
                Task::none()
            }
        }
    }
}

fn usage_color(pct: f64) -> Color {
    if pct >= 80.0 {
        Color::from_rgb(0.70, 0.12, 0.12)
    } else if pct >= 50.0 {
        Color::from_rgb(0.72, 0.55, 0.05)
    } else {
        Color::from_rgb(0.12, 0.55, 0.22)
    }
}
