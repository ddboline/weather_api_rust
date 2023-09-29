use crate::{exponential_retry, get_md5sum, polars_analysis::merge_parquet_files};
use anyhow::{format_err, Error};
use aws_config::SdkConfig;
use aws_sdk_s3::{
    operation::list_objects::ListObjectsOutput, primitives::ByteStream, types::Object as S3Object,
    Client as S3Client,
};
use futures::future::{join_all, try_join_all};
use log::debug;
use rand::{
    distributions::{Alphanumeric, DistString},
    thread_rng,
};
use stack_string::{format_sstr, StackString};
use std::{
    borrow::Borrow,
    collections::{BTreeSet, HashMap, HashSet},
    convert::TryInto,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::Arc,
    time::SystemTime,
};
use time::OffsetDateTime;
use tokio::{fs::File, task::spawn_blocking};

#[derive(Clone)]
pub struct S3Sync {
    s3_client: S3Client,
}

#[derive(Debug, Clone, Eq)]
struct KeyItem {
    key: StackString,
    etag: StackString,
    timestamp: i64,
    size: u64,
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

fn process_s3_item(mut item: S3Object) -> Option<KeyItem> {
    item.key.take().and_then(|key| {
        item.e_tag.take().and_then(|etag| {
            item.last_modified.as_ref().and_then(|last_mod| {
                OffsetDateTime::from_unix_timestamp(last_mod.as_secs_f64() as i64)
                    .ok()
                    .map(|lm| KeyItem {
                        key: key.into(),
                        etag: etag.trim_matches('"').into(),
                        timestamp: lm.unix_timestamp(),
                        size: item.size as u64,
                    })
            })
        })
    })
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

    async fn _get_list_of_keys(&self, bucket: &str) -> Result<Vec<KeyItem>, Error> {
        let mut marker: Option<String> = None;
        let mut list_of_keys = Vec::new();
        loop {
            let mut output = self._list_objects(bucket, marker.as_ref()).await?;
            if let Some(contents) = output.contents.take() {
                if let Some(last) = contents.last() {
                    if let Some(key) = last.key() {
                        marker.replace(key.into());
                    }
                }
                list_of_keys.extend(contents.into_iter().filter_map(process_s3_item));
            }
            if !output.is_truncated {
                break;
            }
        }
        Ok(list_of_keys)
    }

    /// # Errors
    /// Return error if db query fails
    async fn get_list_of_keys(&self, bucket: &str) -> Result<Vec<KeyItem>, Error> {
        let results: Result<Vec<_>, _> =
            exponential_retry(|| async move { self._get_list_of_keys(bucket).await }).await;
        let list_of_keys = results?;
        Ok(list_of_keys)
    }

    /// # Errors
    /// Return error if db query fails
    pub async fn sync_dir(
        &self,
        title: &str,
        local_dir: &Path,
        s3_bucket: &str,
        check_md5sum: bool,
    ) -> Result<StackString, Error> {
        let file_list: Result<Vec<_>, Error> = local_dir
            .read_dir()?
            .filter_map(|dir_line| {
                dir_line.ok().map(|entry| entry.path()).map(|f| {
                    let metadata = fs::metadata(&f)?;
                    let modified: i64 = metadata
                        .modified()?
                        .duration_since(SystemTime::UNIX_EPOCH)?
                        .as_secs()
                        .try_into()?;
                    let size = metadata.len();
                    Ok((f, modified, size))
                })
            })
            .collect();
        let file_list = file_list?;
        let file_set: HashMap<StackString, _> = file_list
            .iter()
            .filter_map(|(f, t, s)| {
                f.file_name()
                    .map(|x| (x.to_string_lossy().as_ref().into(), (*t, *s)))
            })
            .collect();

        let key_list = self.get_list_of_keys(s3_bucket).await?;
        let n_keys = key_list.len();

        let key_set: HashSet<&KeyItem> = key_list.iter().collect();

        let downloaded = {
            let local_dir = local_dir.to_path_buf();
            let s3_bucket: StackString = s3_bucket.into();
            get_downloaded(&key_set, check_md5sum, &file_set, &local_dir, &s3_bucket).await?
        };
        debug!("downloaded {downloaded:?}");
        let downloaded_files: BTreeSet<_> = downloaded
            .iter()
            .map(|(file_name, _)| file_name.clone())
            .collect();
        for (file_name, key) in downloaded {
            debug!("file_name {file_name:?} key {key}");
            self.download_file(&file_name, s3_bucket, &key).await?;
        }
        debug!("downloaded {:?}", downloaded_files);
        let downloaded_files = Arc::new(downloaded_files);

        let key_set = Arc::new(key_set);

        debug!("downloaded {downloaded_files:?}");

        // let uploaded: Vec<_> =
        let futures = file_list.into_iter().map(|(file, tmod, size)| {
            let key_set = key_set.clone();
            let downloaded_files = downloaded_files.clone();
            async move {
                let file_name: StackString = file.file_name()?.to_string_lossy().as_ref().into();
                let mut do_upload = false;
                if let Some(item) = key_set.get(file_name.as_str()) {
                    if tmod != item.timestamp {
                        if check_md5sum {
                            if let Ok(md5) = get_md5sum(&file).await {
                                if item.etag != md5 {
                                    debug!(
                                        "upload md5 {} {} {} {} {}",
                                        file_name, item.etag, md5, item.timestamp, tmod
                                    );
                                    do_upload = true;
                                }
                            }
                        } else if size > item.size {
                            debug!(
                                "upload size {} {} {} {} {}",
                                file_name, item.etag, size, item.timestamp, item.size
                            );
                            do_upload = true;
                        }
                    }
                    if tmod != item.timestamp && check_md5sum {}
                } else {
                    do_upload = true;
                }
                if do_upload {
                    if downloaded_files.contains(&file) {
                        debug!("{file_name} download and upload");
                    }
                    debug!("upload file {}", file_name);
                    Some((file, file_name))
                } else {
                    None
                }
            }
        });
        let uploaded: Vec<_> = join_all(futures).await.into_iter().flatten().collect();
        let uploaded_files: Vec<_> = uploaded
            .iter()
            .map(|(_, filename)| filename.clone())
            .collect();
        for (file, filename) in uploaded {
            self.upload_file(&file, s3_bucket, &filename).await?;
        }
        debug!("uploaded {:?}", uploaded_files);

        let msg = format_sstr!(
            "{} {} s3_bucketnkeys {} uploaded {} downloaded {}",
            title,
            s3_bucket,
            n_keys,
            uploaded_files.len(),
            downloaded_files.len()
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
        let etag = object.e_tag().ok_or_else(|| format_err!("No etag"))?.into();
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
        let object = self
            .s3_client
            .put_object()
            .bucket(bucket)
            .key(key)
            .body(body)
            .send()
            .await?;
        let etag = object.e_tag.ok_or_else(|| format_err!("Missing etag"))?;
        Ok(etag.into())
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

async fn get_downloaded(
    key_list: &HashSet<&KeyItem>,
    check_md5sum: bool,
    file_set: &HashMap<StackString, (i64, u64)>,
    local_dir: &Path,
    s3_bucket: &str,
) -> Result<Vec<(PathBuf, StackString)>, Error> {
    let futures = key_list.iter().map(|item| async move {
        {
            let mut do_download = false;

            if file_set.contains_key(&item.key) {
                let (tmod_, size_) = file_set[&item.key];
                if item.timestamp != tmod_ {
                    if check_md5sum {
                        let file_name = local_dir.join(item.key.as_str());
                        let md5_ = get_md5sum(&file_name).await?;
                        if md5_.as_str() != item.etag.as_str() {
                            debug!(
                                "download md5 {} {} {} {} {} ",
                                item.key, md5_, item.etag, item.timestamp, tmod_
                            );
                            do_download = true;
                        }
                    } else if item.size != size_ {
                        debug!(
                            "download size {} {} {} {} {}",
                            item.key, size_, item.size, item.timestamp, tmod_
                        );
                        do_download = true;
                    }
                }
            } else {
                do_download = true;
            };

            if do_download {
                let file_name = local_dir.join(item.key.as_str());
                debug!("download {} {}", s3_bucket, item.key);
                Ok(Some((file_name, item.key.clone())))
            } else {
                Ok(None)
            }
        }
    });
    let result: Result<Vec<_>, Error> = try_join_all(futures).await;
    Ok(result?.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use anyhow::Error;
    use std::path::Path;
    use tokio::fs::remove_file;

    use crate::s3_sync::S3Sync;

    #[tokio::test]
    #[ignore]
    async fn test_list_keys() -> Result<(), Error> {
        let aws_config = aws_config::load_from_env().await;
        let s3_sync = S3Sync::new(&aws_config);
        let s3_bucket = "aws-athena-query-results-281914939654-us-east-1";
        let s3_key = "064e0d20-19ef-46d7-a1fe-aab556ef2c7e.csv";
        let keys = s3_sync.get_list_of_keys(s3_bucket).await?;
        assert!(keys.len() > 0);
        let local_file = Path::new("/tmp/temp.tmp");
        let tag = s3_sync.download_file(local_file, s3_bucket, s3_key).await?.replace("\"", "");
        assert_eq!(&tag, "af59a680bc9c39776f9657dba46e008f");
        remove_file("/tmp/temp.tmp").await?;
        let local_file = Path::new("/home/ddboline/movie_queue.sql.gz");
        let s3_bucket = "aws-athena-query-results-281914939654-us-east-1";
        let tag = s3_sync.upload_file(local_file, s3_bucket, "movie_queue.sql.gz").await?.replace("\"", "");
        assert_eq!(&tag, "3a9f98b81b5495b4a835b02d32e694de");
        Ok(())
    }
}
