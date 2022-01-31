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

use crate::{
    errors::InternalError,
    model::{
        event::{Event, Filters},
        events::Events,
    },
};
use askama::Template;
use axum::{
    extract::{Extension, Query},
    response::Html,
};
use chrono::{naive, Datelike, NaiveDate};

pub async fn index(
    Extension(events): Extension<Events>,
    Query(filters): Query<Filters>,
) -> Result<Html<String>, InternalError> {
    let events = events.matching(&filters);
    let months = sort_and_group_by_month(events);
    let template = IndexTemplate { filters, months };
    Ok(Html(template.render()?))
}

#[derive(Template)]
#[template(path = "index.html")]
struct IndexTemplate {
    filters: Filters,
    months: Vec<Month>,
}

struct Month {
    /// The first day of the month.
    start: NaiveDate,
    events: Vec<Event>,
}

impl Month {
    pub fn name(&self) -> String {
        self.start.format("%B %Y").to_string()
    }
}

/// Given a list of events in arbitrary order, sort them in ascending order of start date, then group them by starting month.
fn sort_and_group_by_month(mut events: Vec<&Event>) -> Vec<Month> {
    events.sort_by_key(|event| event.start_date);

    let mut months = vec![];
    let mut month = Month {
        start: naive::MIN_DATE,
        events: vec![],
    };
    for event in events {
        if event.start_date.year() == month.start.year()
            && event.start_date.month() == month.start.month()
        {
            month.events.push(event.to_owned());
        } else {
            if !month.events.is_empty() {
                months.push(month);
            }
            month = Month {
                start: NaiveDate::from_ymd(event.start_date.year(), event.start_date.month(), 1),
                events: vec![event.to_owned()],
            };
        }
    }
    if !month.events.is_empty() {
        months.push(month);
    }

    months
}
