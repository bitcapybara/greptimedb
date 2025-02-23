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

use std::sync::Arc;

use object_store::services::Fs;
use object_store::util::{join_dir, with_instrument_layers};
use object_store::ObjectStore;
use snafu::ResultExt;
use store_api::metadata::RegionMetadataRef;

use crate::cache::write_cache::SstUploadRequest;
use crate::cache::CacheManagerRef;
use crate::error::{CleanDirSnafu, DeleteIndexSnafu, DeleteSstSnafu, OpenDalSnafu, Result};
use crate::read::Source;
use crate::sst::file::{FileHandle, FileId, FileMeta};
use crate::sst::location;
use crate::sst::parquet::reader::ParquetReaderBuilder;
use crate::sst::parquet::writer::ParquetWriter;
use crate::sst::parquet::{SstInfo, WriteOptions};

pub type AccessLayerRef = Arc<AccessLayer>;

/// A layer to access SST files under the same directory.
pub struct AccessLayer {
    region_dir: String,
    /// Target object store.
    object_store: ObjectStore,
}

impl std::fmt::Debug for AccessLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccessLayer")
            .field("region_dir", &self.region_dir)
            .finish()
    }
}

impl AccessLayer {
    /// Returns a new [AccessLayer] for specific `region_dir`.
    pub fn new(region_dir: impl Into<String>, object_store: ObjectStore) -> AccessLayer {
        AccessLayer {
            region_dir: region_dir.into(),
            object_store,
        }
    }

    /// Returns the directory of the region.
    pub fn region_dir(&self) -> &str {
        &self.region_dir
    }

    /// Returns the object store of the layer.
    pub fn object_store(&self) -> &ObjectStore {
        &self.object_store
    }

    /// Deletes a SST file (and its index file if it has one) with given file id.
    pub(crate) async fn delete_sst(&self, file_meta: &FileMeta) -> Result<()> {
        let path = location::sst_file_path(&self.region_dir, file_meta.file_id);
        self.object_store
            .delete(&path)
            .await
            .context(DeleteSstSnafu {
                file_id: file_meta.file_id,
            })?;

        if file_meta.inverted_index_available() {
            let path = location::index_file_path(&self.region_dir, file_meta.file_id);
            self.object_store
                .delete(&path)
                .await
                .context(DeleteIndexSnafu {
                    file_id: file_meta.file_id,
                })?;
        }

        Ok(())
    }

    /// Returns a reader builder for specific `file`.
    pub(crate) fn read_sst(&self, file: FileHandle) -> ParquetReaderBuilder {
        ParquetReaderBuilder::new(self.region_dir.clone(), file, self.object_store.clone())
    }

    /// Writes a SST with specific `file_id` and `metadata` to the layer.
    ///
    /// Returns the info of the SST. If no data written, returns None.
    pub(crate) async fn write_sst(
        &self,
        request: SstWriteRequest,
        write_opts: &WriteOptions,
    ) -> Result<Option<SstInfo>> {
        let file_path = location::sst_file_path(&self.region_dir, request.file_id);
        let index_file_path = location::index_file_path(&self.region_dir, request.file_id);
        let region_id = request.metadata.region_id;

        let sst_info = if let Some(write_cache) = request.cache_manager.write_cache() {
            // Write to the write cache.
            write_cache
                .write_and_upload_sst(
                    SstUploadRequest {
                        file_id: request.file_id,
                        metadata: request.metadata,
                        source: request.source,
                        storage: request.storage,
                        upload_path: file_path,
                        index_upload_path: index_file_path,
                        remote_store: self.object_store.clone(),
                    },
                    write_opts,
                )
                .await?
        } else {
            // Write cache is disabled.
            let mut writer =
                ParquetWriter::new(file_path, request.metadata, self.object_store.clone());
            writer.write_all(request.source, write_opts).await?
        };

        // Put parquet metadata to cache manager.
        if let Some(sst_info) = &sst_info {
            if let Some(parquet_metadata) = &sst_info.file_metadata {
                request.cache_manager.put_parquet_meta_data(
                    region_id,
                    request.file_id,
                    parquet_metadata.clone(),
                )
            }
        }

        Ok(sst_info)
    }
}

/// Contents to build a SST.
pub(crate) struct SstWriteRequest {
    pub(crate) file_id: FileId,
    pub(crate) metadata: RegionMetadataRef,
    pub(crate) source: Source,
    pub(crate) cache_manager: CacheManagerRef,
    pub(crate) storage: Option<String>,
}

/// Creates a fs object store with atomic write dir.
pub(crate) async fn new_fs_object_store(root: &str) -> Result<ObjectStore> {
    let atomic_write_dir = join_dir(root, ".tmp/");
    clean_dir(&atomic_write_dir).await?;

    let mut builder = Fs::default();
    builder.root(root).atomic_write_dir(&atomic_write_dir);
    let object_store = ObjectStore::new(builder).context(OpenDalSnafu)?.finish();

    // Add layers.
    let object_store = with_instrument_layers(object_store);
    Ok(object_store)
}

/// Clean the directory.
async fn clean_dir(dir: &str) -> Result<()> {
    if tokio::fs::try_exists(dir)
        .await
        .context(CleanDirSnafu { dir })?
    {
        tokio::fs::remove_dir_all(dir)
            .await
            .context(CleanDirSnafu { dir })?;
    }

    Ok(())
}
