use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use s3::{creds::Credentials, Bucket, Region};
use std::{
    env,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs;

use crate::config::{env_path, env_required};

#[async_trait]
pub trait Storage: Send + Sync {
    async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<()>;
    async fn put_file(&self, key: &str, path: &Path) -> Result<()>;
    async fn get(&self, key: &str) -> Result<Vec<u8>>;
    async fn delete_prefix(&self, prefix: &str) -> Result<()>;
    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>>;
}

pub fn build_storage() -> Result<Arc<dyn Storage>> {
    match env::var("CHATVAULT_BUCKET_PROVIDER")
        .unwrap_or_else(|_| "s3".to_string())
        .as_str()
    {
        "filesystem" => Ok(Arc::new(FilesystemStorage {
            root: env_path("CHATVAULT_BUCKET_ROOT", "/opt/chatvault/archive"),
        })),
        "s3" => Ok(Arc::new(s3_storage()?)),
        other => Err(anyhow!("unsupported CHATVAULT_BUCKET_PROVIDER={other}")),
    }
}

struct S3Storage {
    bucket: Bucket,
}

#[async_trait]
impl Storage for S3Storage {
    async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<()> {
        self.bucket
            .put_object(key, &bytes)
            .await
            .with_context(|| format!("failed to put S3 object {key}"))?;
        Ok(())
    }

    async fn put_file(&self, key: &str, path: &Path) -> Result<()> {
        let mut file = fs::File::open(path).await?;
        self.bucket
            .put_object_stream(&mut file, key)
            .await
            .with_context(|| format!("failed to stream S3 object {key}"))?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        let response = self
            .bucket
            .get_object(key)
            .await
            .with_context(|| format!("failed to get S3 object {key}"))?;
        Ok(response.bytes().to_vec())
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<()> {
        for key in self.list_prefix(prefix).await? {
            self.bucket
                .delete_object(&key)
                .await
                .with_context(|| format!("failed to delete S3 object {key}"))?;
        }
        Ok(())
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let results = self
            .bucket
            .list(prefix.to_string(), None)
            .await
            .with_context(|| format!("failed to list S3 prefix {prefix}"))?;
        Ok(results
            .into_iter()
            .flat_map(|page| page.contents.into_iter().map(|object| object.key))
            .collect())
    }
}

struct FilesystemStorage {
    root: PathBuf,
}

#[async_trait]
impl Storage for FilesystemStorage {
    async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<()> {
        let path = safe_join(&self.root, key)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(path, bytes).await?;
        Ok(())
    }

    async fn put_file(&self, key: &str, path: &Path) -> Result<()> {
        let target = safe_join(&self.root, key)?;
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::copy(path, target).await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Vec<u8>> {
        Ok(fs::read(safe_join(&self.root, key)?).await?)
    }

    async fn delete_prefix(&self, prefix: &str) -> Result<()> {
        let path = safe_join(&self.root, prefix)?;
        if fs::try_exists(&path).await? {
            fs::remove_dir_all(path).await?;
        }
        Ok(())
    }

    async fn list_prefix(&self, prefix: &str) -> Result<Vec<String>> {
        let base = safe_join(&self.root, prefix)?;
        let mut keys = Vec::new();
        if fs::try_exists(&base).await? {
            list_files_recursive(&self.root, &base, &mut keys).await?;
        }
        Ok(keys)
    }
}

fn s3_storage() -> Result<S3Storage> {
    let endpoint = env_required("S3_ENDPOINT")?;
    let bucket_name = env_required("S3_BUCKET")?;
    let access_key = env_required("S3_ACCESS_KEY")?;
    let secret_key = env_required("S3_SECRET_KEY")?;
    let region_name = env::var("S3_REGION").unwrap_or_else(|_| "us-east-1".to_string());
    let credentials = Credentials::new(Some(&access_key), Some(&secret_key), None, None, None)?;
    let region = Region::Custom {
        region: region_name,
        endpoint,
    };
    let bucket = Bucket::new(&bucket_name, region, credentials)?.with_path_style();
    Ok(S3Storage { bucket: *bucket })
}

fn safe_join(root: &Path, key: &str) -> Result<PathBuf> {
    let path = root.join(key).components().collect::<PathBuf>();
    let full = root.join(path);
    if !full.starts_with(root) {
        return Err(anyhow!("path escapes root"));
    }
    Ok(full)
}

async fn list_files_recursive(root: &Path, dir: &Path, keys: &mut Vec<String>) -> Result<()> {
    let mut entries = fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_dir() {
            Box::pin(list_files_recursive(root, &path, keys)).await?;
        } else {
            keys.push(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

pub fn object_key(bucket: &str, file_name: &str) -> Result<String> {
    let key = normalize_prefix(bucket) + file_name.trim_start_matches('/');
    if key.contains("..") {
        return Err(anyhow!("invalid object key {key}"));
    }
    Ok(key)
}

pub fn normalize_prefix(prefix: &str) -> String {
    let trimmed = prefix.trim_matches('/');
    if trimmed.is_empty() {
        String::new()
    } else {
        format!("{trimmed}/")
    }
}
