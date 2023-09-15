// Copyright 2023 Greptime Team
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![feature(assert_matches)]
#![feature(trait_upcasting)]

pub(crate) mod delete;
pub mod error;
pub mod expr_factory;
pub mod frontend;
pub mod heartbeat;
pub(crate) mod insert;
pub mod instance;
pub(crate) mod metrics;
pub(crate) mod region_req_factory;
pub(crate) mod req_convert;
mod script;
mod server;
pub mod service_config;
pub mod statement;
pub mod table;
#[cfg(test)]
pub(crate) mod tests;

pub const MAX_VALUE: &str = "MAXVALUE";
