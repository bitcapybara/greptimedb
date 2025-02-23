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

use async_trait::async_trait;
use common_error::ext::BoxedError;
use common_telemetry::warn;
use futures::{AsyncRead, AsyncWrite};
use index::inverted_index::create::sort::external_provider::ExternalTempFileProvider;
use index::inverted_index::error as index_error;
use index::inverted_index::error::Result as IndexResult;
use snafu::ResultExt;

use crate::error::Result;
use crate::metrics::{
    INDEX_INTERMEDIATE_FLUSH_OP_TOTAL, INDEX_INTERMEDIATE_READ_BYTES_TOTAL,
    INDEX_INTERMEDIATE_READ_OP_TOTAL, INDEX_INTERMEDIATE_SEEK_OP_TOTAL,
    INDEX_INTERMEDIATE_WRITE_BYTES_TOTAL, INDEX_INTERMEDIATE_WRITE_OP_TOTAL,
};
use crate::sst::index::store::InstrumentedStore;
use crate::sst::location::IntermediateLocation;

/// `TempFileProvider` implements `ExternalTempFileProvider`.
/// It uses `InstrumentedStore` to create and read intermediate files.
pub(crate) struct TempFileProvider {
    /// Provides the location of intermediate files.
    location: IntermediateLocation,
    /// Provides access to files in the object store.
    store: InstrumentedStore,
}

#[async_trait]
impl ExternalTempFileProvider for TempFileProvider {
    async fn create(
        &self,
        column_id: &str,
        file_id: &str,
    ) -> IndexResult<Box<dyn AsyncWrite + Unpin + Send>> {
        let path = self.location.file_path(column_id, file_id);
        let writer = self
            .store
            .writer(
                &path,
                &INDEX_INTERMEDIATE_WRITE_BYTES_TOTAL,
                &INDEX_INTERMEDIATE_WRITE_OP_TOTAL,
                &INDEX_INTERMEDIATE_FLUSH_OP_TOTAL,
            )
            .await
            .map_err(BoxedError::new)
            .context(index_error::ExternalSnafu)?;
        Ok(Box::new(writer))
    }

    async fn read_all(
        &self,
        column_id: &str,
    ) -> IndexResult<Vec<Box<dyn AsyncRead + Unpin + Send>>> {
        let column_path = self.location.column_path(column_id);
        let entries = self
            .store
            .list(&column_path)
            .await
            .map_err(BoxedError::new)
            .context(index_error::ExternalSnafu)?;
        let mut readers = Vec::with_capacity(entries.len());

        for entry in entries {
            if entry.metadata().is_dir() {
                warn!("Unexpected entry in index creation dir: {:?}", entry.path());
                continue;
            }

            let reader = self
                .store
                .reader(
                    entry.path(),
                    &INDEX_INTERMEDIATE_READ_BYTES_TOTAL,
                    &INDEX_INTERMEDIATE_READ_OP_TOTAL,
                    &INDEX_INTERMEDIATE_SEEK_OP_TOTAL,
                )
                .await
                .map_err(BoxedError::new)
                .context(index_error::ExternalSnafu)?;
            readers.push(Box::new(reader) as _);
        }

        Ok(readers)
    }
}

impl TempFileProvider {
    /// Creates a new `TempFileProvider`.
    pub fn new(location: IntermediateLocation, store: InstrumentedStore) -> Self {
        Self { location, store }
    }

    /// Removes all intermediate files.
    pub async fn cleanup(&self) -> Result<()> {
        self.store.remove_all(self.location.root_path()).await
    }
}

#[cfg(test)]
mod tests {
    use futures::{AsyncReadExt, AsyncWriteExt};
    use object_store::services::Memory;
    use object_store::ObjectStore;

    use super::*;
    use crate::sst::file::FileId;

    #[tokio::test]
    async fn test_temp_file_provider_basic() {
        let location = IntermediateLocation::new("region_dir", &FileId::random());
        let object_store = ObjectStore::new(Memory::default()).unwrap().finish();
        let store = InstrumentedStore::new(object_store);
        let provider = TempFileProvider::new(location.clone(), store);

        let column_name = "tag0";
        let file_id = "0000000010";
        let mut writer = provider.create(column_name, file_id).await.unwrap();
        writer.write_all(b"hello").await.unwrap();
        writer.flush().await.unwrap();
        writer.close().await.unwrap();

        let file_id = "0000000100";
        let mut writer = provider.create(column_name, file_id).await.unwrap();
        writer.write_all(b"world").await.unwrap();
        writer.flush().await.unwrap();
        writer.close().await.unwrap();

        let column_name = "tag1";
        let file_id = "0000000010";
        let mut writer = provider.create(column_name, file_id).await.unwrap();
        writer.write_all(b"foo").await.unwrap();
        writer.flush().await.unwrap();
        writer.close().await.unwrap();

        let readers = provider.read_all("tag0").await.unwrap();
        assert_eq!(readers.len(), 2);
        for mut reader in readers {
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).await.unwrap();
            assert!(matches!(buf.as_slice(), b"hello" | b"world"));
        }
        let readers = provider.read_all("tag1").await.unwrap();
        assert_eq!(readers.len(), 1);
        let mut reader = readers.into_iter().next().unwrap();
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await.unwrap();
        assert_eq!(buf, b"foo");

        provider.cleanup().await.unwrap();

        assert!(provider
            .store
            .list(location.root_path())
            .await
            .unwrap()
            .is_empty());
    }
}
