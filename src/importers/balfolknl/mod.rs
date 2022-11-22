// Copyright 2022 the dancelist authors.
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

use super::to_fixed_offset;
use crate::model::{
    dancestyle::DanceStyle,
    event::{self, EventTime},
    events::Events,
};
use chrono::TimeZone;
use chrono_tz::Europe::Amsterdam;
use eyre::{bail, eyre, Report};
use icalendar::{
    Calendar, CalendarComponent, CalendarDateTime, Component, DatePerhapsTime, Event, EventLike,
};
use log::{info, warn};

const BANDS: [&str; 31] = [
    "Achterband",
    "Androneda",
    "Artisjok",
    "Aurélien Claranbaux",
    "Beat Bouet Trio",
    "Berkenwerk",
    "BmB",
    "Celts without Borders",
    "Duo Absynthe",
    "Duo Mackie/Hendrix",
    "Duo Roblin-Thebaut",
    "Emelie Waldken",
    "Fahrenheit",
    "Geronimo",
    "Hartwin Dhoore",
    "La Sauterelle",
    "Laouen",
    "Les Bottines Artistiques",
    "Les Zéoles",
    "Madlot",
    "Mieneke",
    "Momiro",
    "Naragonia",
    "Nebel",
    "Nubia",
    "Paracetamol",
    "QuiVive",
    "Swinco",
    "Wilma",
    "Wouter en de Draak",
    "Wouter Kuyper",
];

pub async fn import_events() -> Result<Events, Report> {
    let calendar = reqwest::get("https://www.balfolk.nl/events.ics")
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
        .ok_or_else(|| eyre!("Event {:?} missing summary.", event))?
        .replace("\\,", ",");
    // Remove city from end of summary and use em dash where appropriate.
    let raw_name = summary.rsplitn(2, ',').last().unwrap();
    let name = raw_name.replace(" - ", " — ");

    // Try to skip music workshops.
    if name.starts_with("Muziekstage") {
        info!("Skipping \"{}\" {}", name, url);
        return Ok(None);
    }

    let description = unescape(
        event
            .get_description()
            .ok_or_else(|| eyre!("Event {:?} missing description.", event))?,
    );
    // Remove name from start of description
    let details = description
        .trim_start_matches(&format!("{}, ", raw_name))
        .trim()
        .to_owned();
    let details = if details.is_empty() {
        None
    } else {
        Some(details)
    };

    let time = get_time(event)?;

    let location = event
        .get_location()
        .ok_or_else(|| eyre!("Event {:?} missing location.", event))?;
    let location_parts = location.split("\\, ").collect::<Vec<_>>();
    let city = match location_parts.len() {
        8 => location_parts[3].to_string(),
        4.. => location_parts[2].to_string(),
        _ => {
            warn!("Invalid location \"{}\" for {}", location, url);
            "".to_string()
        }
    };

    let workshop = name.contains("Fundamentals")
        || name.contains("Basis van")
        || name.contains("beginnerslessen")
        || name.contains("danslessen")
        || name.contains("workshop")
        || name.starts_with("Socialles ")
        || name.starts_with("Proefles ")
        || name == "DenneFeest"
        || name == "Folkbal Wilhelmina"
        || description.contains("Dansworkshop")
        || description.contains("Workshopbeschrijving")
        || description.contains("Workshop ")
        || description.contains("dans uitleg")
        || description.contains("dansuitleg")
        || description.contains(" leren ")
        || description.contains("Vooraf dansuitleg")
        || description.contains("de Docent");
    let social = name.contains("Social dance")
        || name.contains("Balfolkbal")
        || name.contains("Avondbal")
        || name.contains("Bal in")
        || name.contains("Balfolk Bal")
        || name.contains("Vuurbal")
        || name.starts_with("Balfolk Wilhelmina")
        || name.starts_with("Fest Noz")
        || name.starts_with("Folkwoods")
        || name.starts_with("Folkbal")
        || name.starts_with("Socialles ")
        || name.starts_with("Verjaardagsbal")
        || name.starts_with("Balfolk Utrecht Bal")
        || name.starts_with("Verjaardagsbal")
        || name == "Balfolk café Nijmegen"
        || name == "DenneFeest"
        || name == "Folkbal Wilhelmina"
        || description.contains("Bal deel");

    let bands = if social {
        BANDS
            .iter()
            .filter_map(|band| {
                if description.contains(band) {
                    Some(band.to_string())
                } else {
                    None
                }
            })
            .collect()
    } else {
        vec![]
    };

    Ok(Some(event::Event {
        name,
        details,
        links: vec![url],
        time,
        country: "Netherlands".to_string(),
        city,
        styles: vec![DanceStyle::Balfolk],
        workshop,
        social,
        bands,
        callers: vec![],
        price: None,
        organisation: Some("balfolk.nl".to_string()),
        cancelled: false,
        source: None,
    }))
}

fn unescape(s: &str) -> String {
    s.replace("\\,", ",")
        .replace("\\;", ";")
        .replace("\\n", "\n")
        .replace("&amp;", "&")
        .replace("&gt;", ">")
        .replace("&lt;", "<")
        .replace("&nbsp;", " ")
}

fn get_time(event: &Event) -> Result<EventTime, Report> {
    let start = event
        .get_start()
        .ok_or_else(|| eyre!("Event {:?} missing start time.", event))?;
    let end = event
        .get_end()
        .ok_or_else(|| eyre!("Event {:?} missing end time.", event))?;
    Ok(match (start, end) {
        (DatePerhapsTime::Date(start_date), DatePerhapsTime::Date(end_date)) => {
            EventTime::DateOnly {
                start_date,
                // iCalendar DTEND is non-inclusive, so subtract one day.
                end_date: end_date.pred_opt().unwrap(),
            }
        }
        (
            DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone {
                date_time: start,
                tzid: start_tzid,
            }),
            DatePerhapsTime::DateTime(CalendarDateTime::WithTimezone {
                date_time: end,
                tzid: end_tzid,
            }),
        ) => {
            if start_tzid != "Europe/Amsterdam" {
                bail!("Unexpected start timezone {}.", start_tzid)
            }
            if end_tzid != "Europe/Amsterdam" {
                bail!("Unexpected end timezone {}.", end_tzid)
            }
            EventTime::DateTime {
                start: to_fixed_offset(
                    Amsterdam
                        .from_local_datetime(&start)
                        .single()
                        .ok_or_else(|| eyre!("Ambiguous datetime for event {:?}", event))?,
                ),
                end: to_fixed_offset(
                    Amsterdam
                        .from_local_datetime(&end)
                        .single()
                        .ok_or_else(|| eyre!("Ambiguous datetime for event {:?}", event))?,
                ),
            }
        }
        _ => bail!("Mismatched start and end times."),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use chrono::FixedOffset;
    use icalendar::Property;

    #[test]
    fn parse_datetime() {
        let start = Property::new("DTSTART", "20220401T190000")
            .add_parameter("TZID", "Europe/Amsterdam")
            .done();
        let end = Property::new("DTEND", "20220401T190000")
            .add_parameter("TZID", "Europe/Amsterdam")
            .done();
        let event = Event::new()
            .append_property(start)
            .append_property(end)
            .done();

        assert_eq!(
            get_time(&event).unwrap(),
            EventTime::DateTime {
                start: FixedOffset::east_opt(7200)
                    .unwrap()
                    .with_ymd_and_hms(2022, 4, 1, 19, 0, 0)
                    .single()
                    .unwrap(),
                end: FixedOffset::east_opt(7200)
                    .unwrap()
                    .with_ymd_and_hms(2022, 4, 1, 19, 0, 0)
                    .single()
                    .unwrap(),
            }
        );
    }
}
