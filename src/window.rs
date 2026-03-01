use cosmic::app::Core;
use cosmic::iced::platform_specific::shell::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::window::Id;
use cosmic::iced::Limits;
use cosmic::iced_runtime::core::window;
use cosmic::widget;
use cosmic::{Action, Element, Task};

use crate::api;

pub const APP_ID: &str = "com.github.mherczeg.claude-usage-applet";
const POLL_SECONDS: u64 = 300;

// ── Model ───────────────────────────────────────────────────────────────

pub struct Window {
    core: Core,
    popup: Option<Id>,
    usage_data: Option<api::UsageData>,
    error: Option<String>,
    loading: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            core: Core::default(),
            popup: None,
            usage_data: None,
            error: None,
            loading: true,
        }
    }
}

// ── Messages ────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub enum Message {
    TogglePopup,
    PopupClosed(Id),
    Tick,
    Refresh,
    UsageLoaded(Result<api::UsageData, String>),
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
        let window = Window {
            core,
            loading: true,
            ..Default::default()
        };

        // Trigger an initial fetch immediately
        let task = Task::future(async {
            let result = api::fetch_usage().await;
            Message::UsageLoaded(result)
        })
        .map(cosmic::Action::from);

        (window, task)
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    // Periodic poll subscription
    fn subscription(&self) -> cosmic::iced::Subscription<Self::Message> {
        cosmic::iced::time::every(std::time::Duration::from_secs(POLL_SECONDS))
            .map(|_| Message::Tick)
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
            Message::Tick | Message::Refresh => {
                self.loading = true;
                return Task::future(async {
                    let result = api::fetch_usage().await;
                    Message::UsageLoaded(result)
                })
                .map(cosmic::Action::from);
            }
            Message::UsageLoaded(result) => {
                self.loading = false;
                match result {
                    Ok(data) => {
                        self.usage_data = Some(data);
                        self.error = None;
                    }
                    Err(e) => {
                        self.error = Some(e);
                    }
                }
            }
        }
        Task::none()
    }

    // Panel icon — shows usage percentage as text
    fn view(&self) -> Element<'_, Self::Message> {
        let label = match &self.usage_data {
            Some(data) => match &data.five_hour {
                Some(limit) => format!("{:.0}%", limit.utilization),
                None => "—%".to_string(),
            },
            None => "…%".to_string(),
        };

        widget::button::custom(widget::text::body(label))
            .on_press(Message::TogglePopup)
            .class(cosmic::theme::Button::AppletIcon)
            .into()
    }

    // Popup with usage details
    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        let mut content = widget::list_column()
            .padding(10)
            .spacing(4);

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

            for (name, limit) in limits {
                if let Some(limit) = limit {
                    let pct = limit.utilization;
                    let resets = api::format_reset_time(&limit.resets_at);
                    content = content.add(widget::settings::item(
                        *name,
                        widget::text::body(format!("{pct:.1}%  ↻ {resets}")),
                    ));
                }
            }
        } else if self.loading {
            content = content.add(widget::settings::item(
                "Status",
                widget::text::body("Loading…"),
            ));
        }

        // Refresh button
        let refresh_label = if self.loading { "Refreshing…" } else { "↻ Refresh" };
        content = content.add(
            widget::button::standard(refresh_label)
                .on_press(Message::Refresh),
        );

        self.core.applet.popup_container(content).into()
    }
}
