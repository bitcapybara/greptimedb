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

//! Sorted strings tables.

use common_base::readable_size::ReadableSize;

pub mod file;
pub mod file_purger;
pub mod index;
pub mod location;
pub mod parquet;
pub(crate) mod version;

/// Default write buffer size, it should be greater than the default minimum upload part of S3 (5mb).
pub const DEFAULT_WRITE_BUFFER_SIZE: ReadableSize = ReadableSize::mb(8);
