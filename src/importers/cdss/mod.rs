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

use super::icalendar_utils::{get_time, unescape};
use crate::model::{dancestyle::DanceStyle, event, events::Events};
use eyre::{eyre, Report};
use icalendar::{Calendar, CalendarComponent, Component, Event, EventLike};

const BANDS: [&str; 4] = [
    "Bunny Bread Bandits",
    "SpringTide",
    "Stomp Rocket",
    "Supertrad",
];
const CALLERS: [&str; 11] = [
    "Alan Rosenthal",
    "Alice Raybourn",
    "Cathy Campbell",
    "Dave Berman",
    "Gaye Fifer",
    "George Marshall",
    "Janine Smith",
    "Lisa Greenleaf",
    "Michael Karchar",
    "Steve Zakon-Anderson",
    "Walter Zagorski",
];

pub async fn import_events() -> Result<Events, Report> {
    let calendar = reqwest::get("https://cdss.org/events/list/?ical=1")
        .await?
        .text()
        .await?
        .parse::<Calendar>()
        .map_err(|e| eyre!("Error parsing iCalendar file: {}", e))?;
    Ok(Events {
        events: calendar
            .iter()
            .filter_map(|component| {
                if let CalendarComponent::Event(event) = component {
                    convert(event).transpose()
                } else {
                    None
                }
            })
            .collect::<Result<_, _>>()?,
    })
}

fn convert(event: &Event) -> Result<Option<event::Event>, Report> {
    let url = event
        .get_url()
        .ok_or_else(|| eyre!("Event {:?} missing url.", event))?
        .to_owned();
    let summary = event
        .get_summary()
        .ok_or_else(|| eyre!("Event {:?} missing summary.", event))?;
    let description = unescape(
        event
            .get_description()
            .ok_or_else(|| eyre!("Event {:?} missing description.", event))?,
    );
    let time = get_time(event)?;

    let categories = event
        .property_value("CATEGORIES")
        .ok_or_else(|| eyre!("Event {:?} missing categories.", event))?
        .split(",")
        .collect::<Vec<_>>();
    let mut styles = Vec::new();
    if categories.contains(&"Online Event") {
        return Ok(None);
    } else if categories.contains(&"Contra Dance") {
        styles.push(DanceStyle::Contra);
    } else if categories.contains(&"English Country Dance") {
        styles.push(DanceStyle::EnglishCountryDance);
    }

    let location = event
        .get_location()
        .ok_or_else(|| eyre!("Event {:?} missing location.", event))?;
    let location_parts = location.split("\\, ").collect::<Vec<_>>();
    let mut country = location_parts[location_parts.len() - 1].to_owned();
    if country == "United States" {
        country = "USA".to_owned();
    }
    let state = Some(location_parts[location_parts.len() - 3].to_owned());
    let city = location_parts[location_parts.len() - 4].to_owned();

    let organisation = Some(
        if let Some(organiser) = event.properties().get("ORGANIZER") {
            let organiser_name = organiser
                .params()
                .get("CN")
                .ok_or_else(|| eyre!("Event {:?} missing organiser name", event))?
                .value();
            organiser_name[1..organiser_name.len() - 1].to_owned()
        } else {
            "CDSS".to_owned()
        },
    );

    let bands = BANDS
        .iter()
        .filter_map(|band| {
            if description.contains(band) || summary.contains(band) {
                Some(band.to_string())
            } else {
                None
            }
        })
        .collect();
    let callers = CALLERS
        .iter()
        .filter_map(|caller| {
            if description.contains(caller) || summary.contains(caller) {
                Some(caller.to_string())
            } else {
                None
            }
        })
        .collect();

    let details = if description.is_empty() {
        None
    } else {
        Some(description)
    };

    Ok(Some(event::Event {
        name: summary.to_owned(),
        details,
        links: vec![url],
        time,
        country,
        state,
        city,
        styles,
        workshop: false,
        social: true,
        bands,
        callers,
        price: None,
        organisation,
        cancelled: false,
        source: None,
    }))
}
