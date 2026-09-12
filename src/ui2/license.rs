use iced::widget::{column, container, row, text, text_input};
use iced::{Element, Length};

use crate::licensing::{LicenseManagerSnapshot, LicenseOperation, LicenseState, NeedsOnlineReason};

use super::app::Message;
use super::theme;
use super::widgets::{body_font, inline_button_maybe, page_button, page_button_maybe, separator};

#[derive(Default)]
pub struct LicensePage {
    pub key: String,
    pub reveal_key: bool,
    pub confirm_deactivation: bool,
}

impl LicensePage {
    pub fn view(&self, snapshot: LicenseManagerSnapshot) -> Element<'_, Message> {
        let mut content = column![].spacing(theme::PAGE_SPACING);
        content = content.push(status_panel(&snapshot));

        if snapshot.dev_mode {
            // The status panel already explains development mode; avoid
            // repeating the same state three times in the page body.
        } else if snapshot.access.is_allowed() || should_offer_online_recovery(&snapshot) {
            content = content.push(self.active_actions(&snapshot));
        } else {
            content = content.push(self.activation(&snapshot));
        }

        content
            .push(separator())
            .push(
                text("Ключ и подписанный офлайн-сертификат хранятся только в зашифрованном хранилище Windows для текущего пользователя.")
                    .size(theme::SMALL_SIZE)
                    .style(|theme| text::Style {
                        color: Some(theme::muted(theme::is_dark(theme))),
                    }),
            )
            .into()
    }

    fn active_actions(&self, snapshot: &LicenseManagerSnapshot) -> Element<'_, Message> {
        let busy = snapshot.operation != LicenseOperation::Idle;
        let mut content = column![].spacing(theme::PAGE_SPACING);
        if let Some(masked) = &snapshot.masked_key {
            content = content.push(row![
                text("Ключ:").size(theme::BODY_SIZE),
                text(masked.clone())
                    .size(theme::BODY_SIZE)
                    .font(super::widgets::mono_font())
            ]);
        }
        content = content.push(
            row![
                page_button_maybe(
                    "Проверить сейчас",
                    theme::Category::Secondary,
                    false,
                    if busy {
                        None
                    } else {
                        Some(Message::LicenseRefresh)
                    },
                ),
                page_button_maybe(
                    "Деактивировать",
                    theme::Category::Destructive,
                    false,
                    if busy {
                        None
                    } else {
                        Some(Message::LicenseConfirmDeactivate)
                    },
                ),
            ]
            .spacing(theme::PAGE_SPACING),
        );
        if self.confirm_deactivation {
            content = content.push(
                container(column![
                    text("Активация на этом компьютере будет освобождена на сервере.")
                        .size(theme::BODY_SIZE),
                    row![
                        page_button(
                            "Отмена",
                            theme::Category::Ghost,
                            false,
                            Message::LicenseCancelDeactivate,
                        ),
                        page_button(
                            "Подтвердить",
                            theme::Category::Destructive,
                            true,
                            Message::LicenseDeactivate,
                        ),
                    ]
                    .spacing(theme::PAGE_SPACING),
                ])
                .padding(theme::SPACE_MD)
                .style(|theme| theme::status_panel_style(theme, theme::Status::Warn)),
            );
        }
        content.into()
    }

    fn activation(&self, snapshot: &LicenseManagerSnapshot) -> Element<'_, Message> {
        let busy = snapshot.operation != LicenseOperation::Idle;
        let ready = !busy && snapshot.configuration_ready;
        let input = text_input("XXXX-XXXX-XXXX-XXXX", &self.key)
            .secure(!self.reveal_key)
            .style(theme::text_input_style)
            .size(theme::BODY_SIZE)
            .on_input_maybe(if ready {
                Some(Message::LicenseKeyChanged)
            } else {
                None
            })
            .width(Length::Fill)
            .padding(theme::SPACE_SM);
        let can_activate = ready && !self.key.trim().is_empty();
        column![
            text("Лицензионный ключ")
                .size(theme::BODY_SIZE)
                .font(body_font(true)),
            row![
                input,
                inline_button_maybe(
                    if self.reveal_key {
                        "Скрыть"
                    } else {
                        "Показать"
                    },
                    theme::Category::Secondary,
                    false,
                    if busy {
                        None
                    } else {
                        Some(Message::LicenseToggleReveal)
                    },
                )
            ]
            .spacing(theme::SPACE_XS),
            row![
                page_button_maybe(
                    "Вставить из буфера",
                    theme::Category::Secondary,
                    false,
                    if ready {
                        Some(Message::LicensePaste)
                    } else {
                        None
                    },
                ),
                page_button_maybe(
                    "Активировать",
                    theme::Category::Primary,
                    true,
                    if can_activate {
                        Some(Message::LicenseActivate)
                    } else {
                        None
                    },
                ),
            ]
            .spacing(theme::PAGE_SPACING),
        ]
        .spacing(theme::PAGE_SPACING)
        .into()
    }
}

fn status_panel(snapshot: &LicenseManagerSnapshot) -> Element<'static, Message> {
    let title = state_title(&snapshot.state, snapshot.dev_mode);
    let status = state_status(&snapshot.state, snapshot.dev_mode);
    let busy = snapshot.operation != LicenseOperation::Idle;
    let mut content = column![text(if busy {
        format!("{title} (выполняется…)")
    } else {
        title.to_owned()
    })
    .size(theme::BODY_SIZE)
    .font(body_font(true))
    .style(move |theme| text::Style {
        color: Some(theme::status_color(status, theme::is_dark(theme))),
    }),]
    .spacing(theme::SPACE_XS);
    if !snapshot.dev_mode {
        if let Some(expiry) = state_expiry(&snapshot.state) {
            content = content.push(
                text(format!("Офлайн-доступ до {}", format_time(expiry)))
                    .size(theme::BODY_SIZE)
                    .style(move |theme| text::Style {
                        color: Some(theme::ink(theme::is_dark(theme))),
                    }),
            );
        }
    }
    if !snapshot.message.is_empty() {
        content = content.push(
            text(snapshot.message.clone())
                .size(theme::SMALL_SIZE)
                .style(move |theme| text::Style {
                    color: Some(theme::ink(theme::is_dark(theme))),
                }),
        );
    }
    container(content)
        .width(Length::Fill)
        .padding(theme::SPACE_MD)
        .style(move |theme| theme::status_panel_style(theme, status))
        .into()
}

fn state_title(state: &LicenseState, dev_mode: bool) -> &'static str {
    if dev_mode {
        return "Режим разработки";
    }
    match state {
        LicenseState::OnlineValid(_) => "Лицензия подтверждена",
        LicenseState::OfflineLease(_) | LicenseState::ServiceUnavailable(_) => {
            "Работа в офлайн-режиме"
        }
        LicenseState::Activating => "Выполняется активация",
        LicenseState::NeedsOnline(NeedsOnlineReason::ClockRollback) => "Требуется проверка времени",
        LicenseState::NeedsOnline(_) => "Требуется подключение",
        LicenseState::Suspended => "Лицензия заблокирована",
        LicenseState::Expired => "Лицензия истекла",
        LicenseState::HardwareMismatch => "Оборудование не подтверждено",
        LicenseState::Tampered => "Данные лицензии повреждены",
        LicenseState::Unlicensed => "Программа не активирована",
    }
}

fn state_status(state: &LicenseState, dev_mode: bool) -> theme::Status {
    if dev_mode {
        return theme::Status::Info;
    }
    match state {
        LicenseState::OnlineValid(_) => theme::Status::Ok,
        LicenseState::OfflineLease(_)
        | LicenseState::ServiceUnavailable(_)
        | LicenseState::Activating => theme::Status::Info,
        LicenseState::NeedsOnline(_) => theme::Status::Warn,
        LicenseState::Suspended
        | LicenseState::Expired
        | LicenseState::HardwareMismatch
        | LicenseState::Tampered => theme::Status::Error,
        LicenseState::Unlicensed => theme::Status::Muted,
    }
}

fn should_offer_online_recovery(snapshot: &LicenseManagerSnapshot) -> bool {
    snapshot.configuration_ready
        && snapshot.masked_key.is_some()
        && matches!(
            snapshot.state,
            LicenseState::NeedsOnline(_)
                | LicenseState::Suspended
                | LicenseState::Expired
                | LicenseState::Unlicensed
        )
}

fn state_expiry(state: &LicenseState) -> Option<i64> {
    match state {
        LicenseState::OnlineValid(lease)
        | LicenseState::OfflineLease(lease)
        | LicenseState::ServiceUnavailable(lease) => Some(lease.offline_valid_until()),
        _ => None,
    }
}

fn format_time(unix_seconds: i64) -> String {
    chrono::DateTime::from_timestamp(unix_seconds, 0)
        .map(|value| {
            value
                .with_timezone(&chrono::Local)
                .format("%d.%m.%Y %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "неизвестно".to_owned())
}
