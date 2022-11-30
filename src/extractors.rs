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

use crate::{errors::InternalError, model::events::Events, AppState};
use axum::{
    async_trait,
    extract::{FromRequestParts, State},
    http::request::Parts,
};
use std::sync::{Arc, Mutex};

#[async_trait]
impl FromRequestParts<AppState> for Events {
    type Rejection = InternalError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let State(events): State<Arc<Mutex<Events>>> =
            State::from_request_parts(parts, state).await?;
        let events = events.lock().unwrap();
        Ok(events.clone())
    }
}
