use crate::embeds;
use twilight_model::{
    channel::message::{
        Embed, MessageFlags,
        component::{
            ActionRow, Button, ButtonStyle, Component, Container, Section, Separator,
            SeparatorSpacingSize, TextDisplay, Thumbnail, UnfurledMediaItem,
        },
    },
    http::interaction::{InteractionResponse, InteractionResponseData, InteractionResponseType},
};

use super::{
    COMPONENT_CYCLE_UPDATE_INTERVAL, COMPONENT_DOWNLOADS, COMPONENT_HELP, COMPONENT_LIBRARY,
    COMPONENT_REFRESH, COMPONENT_SETTINGS, COMPONENT_SOURCES, COMPONENT_STATUS,
    COMPONENT_TOGGLE_AUTO_DOWNLOAD, COMPONENT_UPDATES,
};

pub(super) fn message_response(title: &str, description: &str, color: u32) -> InteractionResponse {
    embed_response(embeds::rich(title, Some(description), color), "")
}

pub(super) fn embed_response(embed: Embed, active_component: &str) -> InteractionResponse {
    InteractionResponse {
        kind: InteractionResponseType::ChannelMessageWithSource,
        data: Some(InteractionResponseData {
            components: Some(component_message(embed, active_component)),
            flags: Some(MessageFlags::IS_COMPONENTS_V2),
            ..InteractionResponseData::default()
        }),
    }
}

fn component_message(embed: Embed, active_component: &str) -> Vec<Component> {
    let mut components = vec![Component::Container(embed_container(embed))];
    if active_component.starts_with("settings:") {
        components.extend(settings_components(active_component));
    } else {
        components.extend(command_components(active_component));
    }
    components
}

fn embed_container(embed: Embed) -> Container {
    let mut components = Vec::new();
    let header = format_header(&embed);
    if let Some(thumbnail) = embed.thumbnail.map(|thumbnail| thumbnail.url) {
        components.push(Component::Section(Section {
            id: None,
            components: vec![text_display(header)],
            accessory: Box::new(Component::Thumbnail(Thumbnail {
                id: None,
                media: UnfurledMediaItem {
                    url: thumbnail,
                    proxy_url: None,
                    height: None,
                    width: None,
                    content_type: None,
                },
                description: embed.title.clone().map(Some),
                spoiler: None,
            })),
        }));
    } else {
        components.push(text_display(header));
    }

    if !embed.fields.is_empty() {
        components.push(separator());
    }
    components.extend(field_components(embed.fields));

    if let Some(footer) = embed.footer {
        components.push(separator());
        components.push(text_display(format!("_{}_", footer.text)));
    }

    Container {
        id: None,
        accent_color: Some(embed.color),
        spoiler: None,
        components,
    }
}

fn format_header(embed: &Embed) -> String {
    match (&embed.title, &embed.description) {
        (Some(title), Some(description)) => {
            format!("## {title}\n{}", normalize_component_text(description))
        }
        (Some(title), None) => format!("## {title}"),
        (None, Some(description)) => normalize_component_text(description),
        (None, None) => "## Manga server".to_owned(),
    }
}

fn normalize_component_text(value: &str) -> String {
    if value.trim().is_empty() {
        "None".to_owned()
    } else {
        value.to_owned()
    }
}

fn text_display(content: String) -> Component {
    Component::TextDisplay(TextDisplay { id: None, content })
}

fn separator() -> Component {
    Component::Separator(Separator {
        id: None,
        divider: Some(true),
        spacing: Some(SeparatorSpacingSize::Large),
    })
}

fn field_components(
    fields: Vec<twilight_model::channel::message::embed::EmbedField>,
) -> Vec<Component> {
    let mut components = Vec::new();
    let mut inline_fields = Vec::new();

    for field in fields {
        if field.inline {
            inline_fields.push(field);
        } else {
            flush_inline_fields(&mut components, &mut inline_fields);
            components.push(format_field(&field));
        }
    }
    flush_inline_fields(&mut components, &mut inline_fields);

    components
}

fn flush_inline_fields(
    components: &mut Vec<Component>,
    inline_fields: &mut Vec<twilight_model::channel::message::embed::EmbedField>,
) {
    if inline_fields.is_empty() {
        return;
    }

    let content = inline_fields
        .drain(..)
        .map(|field| {
            format!(
                "**{}**: {}",
                field.name,
                normalize_component_text(&field.value)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    components.push(text_display(content));
}

fn format_field(field: &twilight_model::channel::message::embed::EmbedField) -> Component {
    text_display(format!(
        "**{}**\n{}",
        field.name,
        normalize_component_text(&field.value)
    ))
}

fn command_components(active_component: &str) -> Vec<Component> {
    vec![
        action_row([
            command_button("Status", COMPONENT_STATUS, active_component),
            command_button("Library", COMPONENT_LIBRARY, active_component),
            command_button("Downloads", COMPONENT_DOWNLOADS, active_component),
        ]),
        action_row([
            command_button("Sources", COMPONENT_SOURCES, active_component),
            command_button("Updates", COMPONENT_UPDATES, active_component),
            command_button("Refresh", COMPONENT_REFRESH, active_component),
            command_button("Help", COMPONENT_HELP, active_component),
        ]),
    ]
}

fn settings_components(active_component: &str) -> Vec<Component> {
    vec![action_row([
        command_button("Overview", COMPONENT_SETTINGS, active_component),
        command_button(
            "Auto-download",
            COMPONENT_TOGGLE_AUTO_DOWNLOAD,
            active_component,
        ),
        command_button(
            "Update interval",
            COMPONENT_CYCLE_UPDATE_INTERVAL,
            active_component,
        ),
    ])]
}

fn action_row<const N: usize>(buttons: [Component; N]) -> Component {
    Component::ActionRow(ActionRow {
        id: None,
        components: Vec::from(buttons),
    })
}

fn command_button(label: &str, custom_id: &str, active_component: &str) -> Component {
    let is_active = custom_id == active_component;
    Component::Button(Button {
        id: None,
        custom_id: Some(custom_id.to_owned()),
        disabled: is_active,
        emoji: None,
        label: Some(label.to_owned()),
        style: button_style(custom_id, is_active),
        url: None,
        sku_id: None,
    })
}

fn button_style(custom_id: &str, is_active: bool) -> ButtonStyle {
    if is_active {
        ButtonStyle::Primary
    } else if matches!(
        custom_id,
        COMPONENT_REFRESH | COMPONENT_TOGGLE_AUTO_DOWNLOAD | COMPONENT_CYCLE_UPDATE_INTERVAL
    ) {
        ButtonStyle::Success
    } else {
        ButtonStyle::Secondary
    }
}
