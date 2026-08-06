//! S3-compatible object storage (RustFS) initialization.

use anyhow::Result;
use aws_sdk_s3::primitives::ByteStream;
use tracing::info;

use crate::core::config::StorageConfig;

pub async fn connect(config: &StorageConfig) -> Result<aws_sdk_s3::Client> {
    let creds = aws_sdk_s3::config::Credentials::new(
        config.access_key_id.clone(),
        config.secret_access_key.expose().to_string(),
        None,
        None,
        "floatctf",
    );

    let s3_config = aws_sdk_s3::Config::builder()
        .region(aws_sdk_s3::config::Region::new(config.region.clone()))
        .endpoint_url(&config.endpoint_url)
        .credentials_provider(creds)
        .force_path_style(true)
        .behavior_version(aws_config::BehaviorVersion::latest())
        .build();

    let client = aws_sdk_s3::Client::from_conf(s3_config);
    info!("Rustfs connected OK");
    ensure_buckets(&client).await?;
    Ok(client)
}

pub async fn ensure_buckets(client: &aws_sdk_s3::Client) -> Result<()> {
    let floatctf_public_bucket_name = "floatctf-public";

    let floatctf_public_bucket = client
        .head_bucket()
        .bucket(floatctf_public_bucket_name)
        .send()
        .await;
    if floatctf_public_bucket.is_err() {
        client
            .create_bucket()
            .bucket(floatctf_public_bucket_name)
            .send()
            .await?;
        let policy = format!(
            r#"{{
                "Version": "2012-10-17",
                "Statement": [
                    {{
                        "Sid": "PublicReadGetObject",
                        "Effect": "Allow",
                        "Principal": "*",
                        "Action": ["s3:GetObject"],
                        "Resource": ["arn:aws:s3:::{}/*"]
                    }}
                ]
            }}"#,
            floatctf_public_bucket_name
        );

        client
            .put_bucket_policy()
            .bucket(floatctf_public_bucket_name)
            .policy(policy)
            .send()
            .await?;
        info!("Bucket {} created", floatctf_public_bucket_name);
    }

    let public_dirs = ["images/", "weapons/", "challenges/"];
    for dir in public_dirs {
        client
            .put_object()
            .bucket(floatctf_public_bucket_name)
            .key(dir)
            .body(ByteStream::from(vec![]))
            .send()
            .await?;
    }
    info!("Public dirs created");

    let floatctf_private_bucket_name = "floatctf-private";
    let floatctf_private_bucket = client
        .head_bucket()
        .bucket(floatctf_private_bucket_name)
        .send()
        .await;

    if floatctf_private_bucket.is_err() {
        client
            .create_bucket()
            .bucket(floatctf_private_bucket_name)
            .send()
            .await?;
        info!("Bucket {} created", floatctf_private_bucket_name);
    }

    let private_dirs = ["writeups"];
    for dir in private_dirs {
        client
            .put_object()
            .bucket(floatctf_private_bucket_name)
            .key(dir)
            .body(ByteStream::from(vec![]))
            .send()
            .await?;
    }
    info!("Private dirs created");

    Ok(())
}
