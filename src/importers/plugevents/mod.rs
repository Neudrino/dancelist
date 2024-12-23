// Copyright 2023 the dancelist authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

mod types;

use self::types::{Event, EventFormat, EventList};
use crate::model::{
    dancestyle::DanceStyle,
    event::{self, EventTime},
    events::Events,
};
use chrono::Timelike;
use eyre::{eyre, Report};

pub async fn events(token: &str) -> Result<Vec<Event>, Report> {
    let json = reqwest::get(format!(
        "https://api1.plug.events/api1/embed/embed1?token={}",
        token
    ))
    .await?
    .text()
    .await?;
    let events: EventList = serde_json::from_str(&json)?;
    Ok(events.events)
}

pub async fn import_events(token: &str) -> Result<Events, Report> {
    let events = events(token).await?;
    let style = DanceStyle::Balfolk;

    Ok(Events {
        events: events
            .iter()
            .filter_map(|event| convert(event, style).transpose())
            .collect::<Result<_, _>>()?,
    })
}

fn convert(event: &Event, style: DanceStyle) -> Result<Option<event::Event>, Report> {
    let Some(venue_locale) = &event.venue_locale else {
        eprintln!("Event \"{}\" has no venueLocale, skipping.", event.name);
        return Ok(None);
    };
    let locale_parts: Vec<_> = venue_locale.split(", ").collect();
    let country = locale_parts
        .last()
        .ok_or_else(|| eyre!("venueLocale only has one part: \"{}\"", venue_locale))?
        .to_string();

    let city = if locale_parts.len() > 3 {
        locale_parts[1]
    } else {
        locale_parts[0]
    }
    .to_string();

    let mut workshop = false;
    let mut social = false;
    for subinterest in event.subinterests.clone().unwrap_or_default() {
        match subinterest {
            EventFormat::Bal
            | EventFormat::Balfolk
            | EventFormat::BalfolkNL
            | EventFormat::Folkbal
            | EventFormat::FolkBal => {
                social = true;
            }
            EventFormat::Advanced
            | EventFormat::Class
            | EventFormat::Course
            | EventFormat::DanceClass
            | EventFormat::Dansles
            | EventFormat::Event
            | EventFormat::Les
            | EventFormat::Learning
            | EventFormat::LessonSeries
            | EventFormat::Intensive => {
                workshop = true;
            }
            EventFormat::Festival => {
                workshop = true;
                social = true;
            }
            EventFormat::Meeting => {
                social = true;
            }
            EventFormat::Organiser => {}
            EventFormat::Dansavond
            | EventFormat::LiveMusic
            | EventFormat::LiveMuziek
            | EventFormat::Party
            | EventFormat::Social
            | EventFormat::SocialDancing => {
                social = true;
            }
            EventFormat::Practica => {
                social = true;
            }
            EventFormat::SocialClass | EventFormat::Sociales => {
                workshop = true;
                social = true;
            }
            EventFormat::MusicClass | EventFormat::Musiekles | EventFormat::Teacher => {}
        }
    }
    if event.name.contains("warsztatów") || event.description.contains("warsztaty") {
        workshop = true;
    }

    Ok(Some(event::Event {
        name: event.name.clone(),
        details: Some(event.description.clone()),
        links: vec![event.plug_url.clone()],
        time: EventTime::DateTime {
            start: event
                .start_date_time_iso
                .with_timezone(&event.timezone)
                .fixed_offset()
                .with_second(0)
                .unwrap()
                .with_nanosecond(0)
                .unwrap(),
            end: event
                .end_date_time_iso
                .with_timezone(&event.timezone)
                .fixed_offset(),
        },
        country,
        state: None,
        city,
        styles: vec![style],
        workshop,
        social,
        bands: vec![],
        callers: vec![],
        price: format_price(event),
        organisation: event.published_by_name.as_deref().map(fix_organisation),
        cancelled: false,
        source: None,
    }))
}

fn fix_organisation(published_by_name: &str) -> String {
    match published_by_name {
        "Chata Numinosum" => "Numinosum".to_string(),
        _ => published_by_name.to_string(),
    }
}

fn format_price(event: &Event) -> Option<String> {
    if event.is_free {
        Some("free".to_string())
    } else {
        event.price_display.as_ref().map(|price| {
            let mut price = price.replace(" ", "");
            let currency = price.chars().next().unwrap();
            if "$£€".contains(currency) {
                price = price.replace("-", &format!("-{}", currency));
            }
            price
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_price() {
        assert_eq!(format_price(&Event::default()), None);
        assert_eq!(
            format_price(&Event {
                price_display: Some("€ 10".to_string()),
                ..Default::default()
            }),
            Some("€10".to_string())
        );
        assert_eq!(
            format_price(&Event {
                price_display: Some("€ 5-23".to_string()),
                ..Default::default()
            }),
            Some("€5-€23".to_string())
        );
    }
}
