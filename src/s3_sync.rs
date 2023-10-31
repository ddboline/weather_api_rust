use crate::{exponential_retry, get_md5sum, polars_analysis::merge_parquet_files};
use anyhow::{format_err, Error};
use aws_config::SdkConfig;
use aws_sdk_s3::{
    operation::list_objects::ListObjectsOutput, primitives::ByteStream, types::Object as S3Object,
    Client as S3Client,
};
use futures::TryStreamExt;
use log::debug;
use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng,
};
use stack_string::{format_sstr, StackString};
use std::{
    borrow::Borrow,
    cmp::Ordering,
    convert::{TryFrom, TryInto},
    fs,
    hash::{Hash, Hasher},
    path::Path,
    time::SystemTime,
};
use tokio::{fs::File, task::spawn_blocking};

use crate::{model::KeyItemCache, pgpool::PgPool};

#[derive(Clone)]
pub struct S3Sync {
    s3_client: S3Client,
}

#[derive(Debug, Clone, Eq)]
pub struct KeyItem {
    key: StackString,
    etag: StackString,
    timestamp: i64,
    size: u64,
}

impl KeyItem {
    fn from_s3_object(mut item: S3Object) -> Option<Self> {
        let key = item.key.take()?.into();
        let etag = item.e_tag.take()?.trim_matches('"').into();
        let timestamp = item.last_modified.as_ref()?.as_secs_f64() as i64;

        Some(Self {
            key,
            etag,
            timestamp,
            size: item.size as u64,
        })
    }
}

impl From<KeyItemCache> for KeyItem {
    fn from(value: KeyItemCache) -> Self {
        Self {
            key: value.s3_key,
            etag: value.etag,
            timestamp: value.s3_timestamp,
            size: value.s3_size as u64,
        }
    }
}

impl TryFrom<KeyItem> for KeyItemCache {
    type Error = Error;
    fn try_from(value: KeyItem) -> Result<Self, Self::Error> {
        Ok(Self {
            s3_key: value.key,
            etag: value.etag,
            s3_timestamp: value.timestamp,
            s3_size: value.size.try_into()?,
            has_local: false,
            has_remote: false,
        })
    }
}

impl PartialEq for KeyItem {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl Hash for KeyItem {
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
        self.key.hash(state);
    }
}

impl Borrow<str> for &KeyItem {
    fn borrow(&self) -> &str {
        self.key.as_str()
    }
}

impl Default for S3Sync {
    fn default() -> Self {
        let config = SdkConfig::builder().build();
        Self::new(&config)
    }
}

impl S3Sync {
    #[must_use]
    pub fn new(config: &SdkConfig) -> Self {
        Self {
            s3_client: S3Client::from_conf(config.into()),
        }
    }

    async fn _list_objects(
        &self,
        bucket: &str,
        marker: Option<impl AsRef<str>>,
    ) -> Result<ListObjectsOutput, Error> {
        let mut builder = self.s3_client.list_objects().bucket(bucket);
        if let Some(marker) = marker {
            builder = builder.marker(marker.as_ref());
        }
        builder.send().await.map_err(Into::into)
    }

    async fn _get_and_process_keys(&self, bucket: &str, pool: &PgPool) -> Result<usize, Error> {
        let mut marker: Option<String> = None;
        let mut nkeys = 0;
        loop {
            let mut output = self._list_objects(bucket, marker.as_ref()).await?;
            if let Some(contents) = output.contents.take() {
                if let Some(last) = contents.last() {
                    if let Some(key) = last.key() {
                        marker.replace(key.into());
                    }
                }
                for object in contents {
                    if let Some(key) = KeyItem::from_s3_object(object) {
                        if let Some(mut key_item) = KeyItemCache::get_by_key(pool, &key.key).await?
                        {
                            key_item.has_remote = true;
                            if key.timestamp != key_item.s3_timestamp && key.etag != key_item.etag {
                                let key_size: i64 = key.size.try_into()?;
                                match key_size.cmp(&key_item.s3_size) {
                                    Ordering::Greater => {
                                        key_item = key.try_into()?;
                                        key_item.has_remote = true;
                                    }
                                    Ordering::Less => {
                                        key_item.has_remote = false;
                                    }
                                    Ordering::Equal => {}
                                }
                            }
                            key_item.insert(pool).await?;
                        } else {
                            let mut key_item: KeyItemCache = key.try_into()?;
                            key_item.has_remote = true;
                            key_item.insert(pool).await?;
                        };
                        nkeys += 1;
                    }
                }
            }
            if !output.is_truncated {
                break;
            }
        }
        Ok(nkeys)
    }

    async fn get_and_process_keys(&self, bucket: &str, pool: &PgPool) -> Result<usize, Error> {
        let result: Result<usize, _> =
            exponential_retry(|| async move { self._get_and_process_keys(bucket, pool).await })
                .await;
        result.map_err(Into::into)
    }

    async fn process_files(&self, local_dir: &Path, pool: &PgPool) -> Result<(), Error> {
        for dir_line in local_dir.read_dir()? {
            let entry = dir_line?;
            let f = entry.path();
            let metadata = fs::metadata(&f)?;
            let modified: i64 = metadata
                .modified()?
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_secs()
                .try_into()?;
            let size: i64 = metadata.len().try_into()?;
            if let Some(file_name) = f.file_name() {
                let key: StackString = file_name.to_string_lossy().as_ref().into();
                if let Some(mut key_item) = KeyItemCache::get_by_key(pool, &key).await? {
                    if modified != key_item.s3_timestamp && size > key_item.s3_size {
                        let etag = get_md5sum(&f).await?;
                        if etag != key_item.etag {
                            key_item.has_local = true;
                            key_item.has_remote = false;
                            key_item.insert(pool).await?;
                        }
                    }
                } else {
                    let etag = get_md5sum(&f).await?;
                    KeyItemCache {
                        s3_key: key,
                        etag,
                        s3_timestamp: modified,
                        s3_size: size,
                        has_local: true,
                        has_remote: false,
                    }
                    .insert(pool)
                    .await?;
                };
            }
        }
        Ok(())
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn sync_dir(
        &self,
        title: &str,
        local_dir: &Path,
        s3_bucket: &str,
        pool: &PgPool,
    ) -> Result<StackString, Error> {
        self.process_files(local_dir, pool).await?;
        let n_keys = self.get_and_process_keys(s3_bucket, pool).await?;

        let mut number_uploaded = 0;
        let mut number_downloaded = 0;

        let mut stream = Box::pin(KeyItemCache::get_files(pool, true, false).await?);

        while let Some(mut key_item) = stream.try_next().await? {
            let local_file = local_dir.join(&key_item.s3_key);
            key_item.etag = self
                .download_file(&local_file, s3_bucket, &key_item.s3_key)
                .await?;
            number_downloaded += 1;
            key_item.has_local = true;
            key_item.insert(pool).await?;
        }

        let mut stream = Box::pin(KeyItemCache::get_files(pool, false, true).await?);

        while let Some(mut key_item) = stream.try_next().await? {
            let local_file = local_dir.join(&key_item.s3_key);
            if !local_file.exists() {
                key_item.has_local = false;
                key_item.insert(pool).await?;
                continue;
            }
            key_item.etag = self
                .upload_file(&local_file, s3_bucket, &key_item.s3_key)
                .await?;
            number_uploaded += 1;
            key_item.has_remote = true;
            key_item.insert(pool).await?;
        }

        let msg = format_sstr!(
            "{} {} s3_bucketnkeys {} uploaded {} downloaded {}",
            title,
            s3_bucket,
            n_keys,
            number_uploaded,
            number_downloaded,
        );
        Ok(msg)
    }

    async fn _download_to_file(
        &self,
        bucket: &str,
        key: &str,
        path: &Path,
    ) -> Result<StackString, Error> {
        let object = self
            .s3_client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await?;
        let etag = object
            .e_tag()
            .ok_or_else(|| format_err!("No etag"))?
            .trim_matches('"')
            .into();
        let body = object.body;
        let mut f = File::create(path).await?;
        tokio::io::copy(&mut body.into_async_read(), &mut f).await?;
        Ok(etag)
    }

    /// # Errors
    /// Return error if db query fails
    async fn download_file(
        &self,
        local_file: &Path,
        s3_bucket: &str,
        s3_key: &str,
    ) -> Result<StackString, Error> {
        let tmp_path = {
            let mut rng = thread_rng();
            let rand_str = Alphanumeric.sample_string(&mut rng, 8);
            local_file.with_file_name(format_sstr!(".tmp_{rand_str}"))
        };
        let etag: Result<StackString, Error> = exponential_retry(|| {
            let tmp_path = tmp_path.clone();
            async move { self._download_to_file(s3_bucket, s3_key, &tmp_path).await }
        })
        .await;
        let output = local_file.to_path_buf();
        debug!("input {tmp_path:?} output {output:?}");
        if output.exists() {
            let input_md5 = get_md5sum(&tmp_path).await?;
            let output_md5 = get_md5sum(&output).await?;
            if input_md5 != output_md5 {
                let result: Result<(), Error> = spawn_blocking(move || {
                    merge_parquet_files(&tmp_path, &output)?;
                    fs::remove_file(&tmp_path).map_err(Into::into)
                })
                .await?;
                result?;
            }
        } else {
            tokio::fs::rename(&tmp_path, &output).await?;
        }
        etag
    }

    async fn _upload_file(
        &self,
        bucket: &str,
        key: &str,
        path: &Path,
    ) -> Result<StackString, Error> {
        let body = ByteStream::read_from().path(path).build().await?;
        let etag = self
            .s3_client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(body)
            .send()
            .await?
            .e_tag
            .ok_or_else(|| format_err!("Missing etag"))?
            .trim_matches('"')
            .into();
        Ok(etag)
    }

    /// # Errors
    /// Return error if db query fails
    async fn upload_file(
        &self,
        local_file: &Path,
        s3_bucket: &str,
        s3_key: &str,
    ) -> Result<StackString, Error> {
        exponential_retry(|| async move { self._upload_file(s3_bucket, s3_key, local_file).await })
            .await
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use futures::TryStreamExt;

    use crate::{config::Config, model::KeyItemCache, pgpool::PgPool, s3_sync::S3Sync};

    #[tokio::test]
    #[ignore]
    async fn test_process_files_and_keys() -> Result<(), Error> {
        let aws_config = aws_config::load_from_env().await;
        let s3_sync = S3Sync::new(&aws_config);
        let config = Config::init_config(None)?;
        let db_url = config.database_url.as_ref().unwrap();
        let pool = PgPool::new(db_url);

        s3_sync.process_files(&config.cache_dir, &pool).await?;
        s3_sync
            .get_and_process_keys(&config.s3_bucket, &pool)
            .await?;

        KeyItemCache::get_files(&pool, true, false)
            .await?
            .try_for_each(|key_item| async move {
                println!("upload {}", key_item.s3_key);
                Ok(())
            })
            .await?;

        KeyItemCache::get_files(&pool, false, true)
            .await?
            .try_for_each(|key_item| async move {
                println!("download {}", key_item.s3_key);
                Ok(())
            })
            .await?;
        Ok(())
    }
}
