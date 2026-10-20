#![cfg(feature = "blocking")]

use std::io::Cursor;

use bytes::Bytes;
use http::HeaderValue;
use http::StatusCode;

use s3::{
    AddressingStyle, Error,
    types::{
        ApplyServerSideEncryptionByDefault, BucketCorsConfiguration, BucketCorsRule,
        BucketEncryptionConfiguration, BucketEncryptionRule, BucketLifecycleConfiguration,
        BucketLifecycleRule, BucketLifecycleStatus, BucketPublicAccessBlockConfiguration,
        BucketTagging, BucketVersioningConfiguration, BucketVersioningStatus, CorsMethod,
        SseAlgorithm, Tag,
    },
};

mod common;

fn is_not_found(err: &Error) -> bool {
    err.status() == Some(StatusCode::NOT_FOUND)
}

fn is_optional_unsupported(err: &Error) -> bool {
    common::is_unsupported(err) || matches!(err.status(), Some(StatusCode::METHOD_NOT_ALLOWED))
}

#[test]
fn s3_compat_blocking_put_get_delete_roundtrip() -> Result<(), Error> {
    let Some(cfg) = common::load_config()? else {
        return Ok(());
    };

    let client = common::build_blocking_client(&cfg, AddressingStyle::Auto)?;
    common::with_bucket_blocking(&client, "s3-it-blocking-", |bucket| {
        let key = "hello.txt";
        let body = Bytes::from_static(b"hello");
        client
            .objects()
            .put(&bucket, key)
            .body_bytes(body.clone())
            .send()?;

        let head = client.objects().head(&bucket, key).send()?;
        assert_eq!(head.content_length, Some(5));

        let range = client
            .objects()
            .get(&bucket, key)
            .range_bytes(0, 3)?
            .send()?
            .bytes()?;
        assert_eq!(range, Bytes::from_static(b"hell"));

        let got = client.objects().get(&bucket, key).send()?.bytes()?;
        assert_eq!(got, body);

        let mut written = Vec::new();
        let n = client
            .objects()
            .get(&bucket, key)
            .send()?
            .write_to(&mut written)?;
        assert_eq!(n, body.len() as u64);
        assert_eq!(Bytes::from(written), body);

        let copied = "copied.txt";
        client
            .objects()
            .copy(&bucket, key, &bucket, copied)
            .send()?;
        let got = client.objects().get(&bucket, copied).send()?.bytes()?;
        assert_eq!(got, body);

        for k in ["a/1.txt", "a/2.txt", "a/3.txt", "b/1.txt", "root.txt"] {
            client
                .objects()
                .put(&bucket, k)
                .body_bytes(Bytes::from_static(b"x"))
                .send()?;
        }

        let mut keys = Vec::new();
        let pager = client
            .objects()
            .list_v2(&bucket)
            .prefix("a/")?
            .max_keys(2)?
            .pager();
        for page in pager {
            let page = page?;
            keys.extend(page.contents.into_iter().map(|o| o.key));
        }
        keys.sort();
        assert_eq!(keys, vec!["a/1.txt", "a/2.txt", "a/3.txt"]);

        let out = client.objects().list_v2(&bucket).delimiter("/")?.send()?;
        assert!(out.common_prefixes.iter().any(|p| p == "a/"));
        assert!(out.common_prefixes.iter().any(|p| p == "b/"));
        assert!(out.contents.iter().any(|o| o.key == "root.txt"));

        match client.objects().get(&bucket, "does-not-exist").send() {
            Ok(_) => panic!("expected not found error"),
            Err(Error::Api(error)) => assert_eq!(error.status, StatusCode::NOT_FOUND),
            Err(other) => panic!("expected api error, got {other:?}"),
        }

        let presigned = client.objects().presign_get(&bucket, key).build()?;
        let mut req = ureq::agent().get(presigned.url.as_str());
        for (name, value) in presigned.headers.iter() {
            let Ok(value_str) = value.to_str() else {
                continue;
            };
            req = req.header(name.as_str(), value_str);
        }
        let resp = req
            .call()
            .map_err(|e| Error::transport("presigned request failed", Some(Box::new(e))))?;
        assert!(resp.status().is_success());

        client
            .objects()
            .delete_objects(&bucket)
            .objects([
                key, copied, "a/1.txt", "a/2.txt", "a/3.txt", "b/1.txt", "root.txt",
            ])?
            .send()?;
        Ok(())
    })
}

#[test]
fn s3_compat_blocking_virtual_hosted_put_get_delete_roundtrip() -> Result<(), Error> {
    let Some(cfg) = common::load_config()? else {
        return Ok(());
    };

    let Some(endpoint) = common::virtual_hosted_endpoint(&cfg.endpoint)? else {
        return Ok(());
    };

    let bucket_client = common::build_blocking_client(&cfg, AddressingStyle::Path)?;
    let vhost_client = s3::BlockingClient::builder(&endpoint)?
        .region(cfg.region.as_str())
        .auth(cfg.auth.clone())
        .addressing_style(AddressingStyle::VirtualHosted)
        .max_attempts(1)
        .build()?;

    common::with_bucket_blocking(&bucket_client, "s3-it-blocking-vhost-", |bucket| {
        let key = "a+b.txt";
        let body = Bytes::from_static(b"hello");
        vhost_client
            .objects()
            .put(&bucket, key)
            .body_bytes(body.clone())
            .send()?;

        let got = vhost_client.objects().get(&bucket, key).send()?.bytes()?;
        assert_eq!(got, body);

        vhost_client.objects().delete(&bucket, key).send()?;
        Ok(())
    })
}

#[test]
fn s3_compat_blocking_put_reader_roundtrip() -> Result<(), Error> {
    let Some(cfg) = common::load_config()? else {
        return Ok(());
    };

    let client = common::build_blocking_client(&cfg, AddressingStyle::Path)?;
    common::with_bucket_blocking(&client, "s3-it-blocking-reader-", |bucket| {
        let key = "reader.txt";
        let body = Bytes::from_static(b"streamed body");

        client
            .objects()
            .put(&bucket, key)
            .body_reader_sized(Cursor::new(body.clone()), body.len() as u64)
            .send()?;

        let got = client.objects().get(&bucket, key).send()?.bytes()?;
        assert_eq!(got, body);
        Ok(())
    })
}

#[test]
fn s3_compat_blocking_get_range_and_conditions() -> Result<(), Error> {
    let Some(cfg) = common::load_config()? else {
        return Ok(());
    };

    let client = common::build_blocking_client(&cfg, AddressingStyle::Path)?;
    common::with_bucket_blocking(&client, "s3-it-blocking-range-", |bucket| {
        let key = "range.txt";
        let body = Bytes::from_static(b"hello world");
        let put = client
            .objects()
            .put(&bucket, key)
            .content_type("text/plain")?
            .body_bytes(body.clone())
            .send()?;
        let etag = put
            .etag
            .ok_or_else(|| Error::decode("missing etag", None))?;

        let got = client
            .objects()
            .get(&bucket, key)
            .range_bytes(0, 4)?
            .send()?
            .bytes()?;
        assert_eq!(got, Bytes::from_static(b"hello"));

        let ok = client
            .objects()
            .get(&bucket, key)
            .if_match(etag.clone())?
            .send()?
            .bytes()?;
        assert_eq!(ok, body);

        let strict = common::strict();
        match client
            .objects()
            .get(&bucket, key)
            .if_none_match(etag)?
            .send()
        {
            Ok(out) => {
                if strict {
                    panic!("expected not modified");
                }
                let got = out.bytes()?;
                assert_eq!(got, body);
            }
            Err(Error::Api(error)) if strict => {
                let status = error.status;
                assert_eq!(status, StatusCode::NOT_MODIFIED)
            }
            Err(Error::Api(error)) => {
                let status = error.status;
                assert!(matches!(
                    status,
                    StatusCode::NOT_MODIFIED
                        | StatusCode::PRECONDITION_FAILED
                        | StatusCode::BAD_REQUEST
                ))
            }
            Err(other) => panic!("expected api error, got {other:?}"),
        }

        client.objects().delete(&bucket, key).send()?;
        Ok(())
    })
}

#[test]
fn s3_compat_blocking_list_buckets_and_bucket_configs() -> Result<(), Error> {
    let Some(cfg) = common::load_config()? else {
        return Ok(());
    };

    let client = common::build_blocking_client(&cfg, AddressingStyle::Auto)?;
    common::with_bucket_blocking(&client, "s3-it-blocking-bucket-", |bucket| {
        let _ = client.buckets().head(&bucket).send()?;

        let buckets = client.buckets().list().send()?;
        assert!(buckets.buckets.iter().any(|b| b.name == bucket));

        let versioning = BucketVersioningConfiguration {
            status: Some(BucketVersioningStatus::Enabled),
            mfa_delete: None,
        };
        match client
            .buckets()
            .put_versioning(&bucket)
            .configuration(versioning)?
            .send()
        {
            Ok(_) => match client.buckets().get_versioning(&bucket).send() {
                Ok(got) => assert_eq!(got.status, Some(BucketVersioningStatus::Enabled)),
                Err(err) if is_optional_unsupported(&err) => {}
                Err(err) => return Err(err),
            },
            Err(err) if is_optional_unsupported(&err) => {}
            Err(err) => return Err(err),
        }

        let tagging = BucketTagging {
            tags: vec![Tag {
                key: "env".to_string(),
                value: "test".to_string(),
            }],
        };
        match client
            .buckets()
            .put_tagging(&bucket)
            .tagging(tagging)?
            .send()
        {
            Ok(_) => {
                match client.buckets().get_tagging(&bucket).send() {
                    Ok(got) => {
                        assert!(got.tags.iter().any(|t| t.key == "env" && t.value == "test"))
                    }
                    Err(err) if is_optional_unsupported(&err) => {}
                    Err(err) => return Err(err),
                }

                match client.buckets().delete_tagging(&bucket).send() {
                    Ok(_) => {}
                    Err(err) if is_optional_unsupported(&err) => {}
                    Err(err) => return Err(err),
                }

                match client.buckets().get_tagging(&bucket).send() {
                    Ok(_) => {}
                    Err(err) if is_not_found(&err) => {}
                    Err(err) if is_optional_unsupported(&err) => {}
                    Err(err) => return Err(err),
                }
            }
            Err(err) if is_optional_unsupported(&err) => {}
            Err(err) => return Err(err),
        }

        let cors = BucketCorsConfiguration {
            rules: vec![BucketCorsRule {
                id: Some("rule-1".to_string()),
                allowed_origins: vec!["*".to_string()],
                allowed_methods: vec![CorsMethod::Get, CorsMethod::Put],
                allowed_headers: vec!["*".to_string()],
                expose_headers: Vec::new(),
                max_age_seconds: Some(3600),
            }],
        };
        match client
            .buckets()
            .put_cors(&bucket)
            .configuration(cors)?
            .send()
        {
            Ok(_) => {
                match client.buckets().get_cors(&bucket).send() {
                    Ok(got) => assert!(!got.rules.is_empty()),
                    Err(err) if is_optional_unsupported(&err) => {}
                    Err(err) => return Err(err),
                }

                match client.buckets().delete_cors(&bucket).send() {
                    Ok(_) => {}
                    Err(err) if is_optional_unsupported(&err) => {}
                    Err(err) => return Err(err),
                }

                match client.buckets().get_cors(&bucket).send() {
                    Ok(_) => {}
                    Err(err) if is_not_found(&err) => {}
                    Err(err) if is_optional_unsupported(&err) => {}
                    Err(err) => return Err(err),
                }
            }
            Err(err) if is_optional_unsupported(&err) => {}
            Err(err) => return Err(err),
        }

        let lifecycle = BucketLifecycleConfiguration {
            rules: vec![BucketLifecycleRule {
                id: Some("rule-1".to_string()),
                status: BucketLifecycleStatus::Enabled,
                prefix: Some("logs/".to_string()),
                expiration_days: Some(1),
                expiration_date: None,
            }],
        };
        match client
            .buckets()
            .put_lifecycle(&bucket)
            .configuration(lifecycle)?
            .send()
        {
            Ok(_) => {
                match client.buckets().get_lifecycle(&bucket).send() {
                    Ok(got) => assert!(!got.rules.is_empty()),
                    Err(err) if is_optional_unsupported(&err) => {}
                    Err(err) => return Err(err),
                }

                match client.buckets().delete_lifecycle(&bucket).send() {
                    Ok(_) => {}
                    Err(err) if is_optional_unsupported(&err) => {}
                    Err(err) => return Err(err),
                }

                match client.buckets().get_lifecycle(&bucket).send() {
                    Ok(_) => {}
                    Err(err) if is_not_found(&err) => {}
                    Err(err) if is_optional_unsupported(&err) => {}
                    Err(err) => return Err(err),
                }
            }
            Err(err) if is_optional_unsupported(&err) => {}
            Err(err) => return Err(err),
        }

        let encryption = BucketEncryptionConfiguration {
            rules: vec![BucketEncryptionRule {
                apply: ApplyServerSideEncryptionByDefault {
                    sse_algorithm: SseAlgorithm::Aes256,
                    kms_master_key_id: None,
                },
                bucket_key_enabled: Some(false),
            }],
        };

        match client
            .buckets()
            .put_encryption(&bucket)
            .configuration(encryption)?
            .send()
        {
            Ok(_) => {
                let got = client.buckets().get_encryption(&bucket).send()?;
                assert!(!got.rules.is_empty());
                match client.buckets().delete_encryption(&bucket).send() {
                    Ok(_) => {}
                    Err(err) if common::is_unsupported(&err) => {}
                    Err(err) => return Err(err),
                }
            }
            Err(err) if common::is_unsupported(&err) => {}
            Err(err)
                if matches!(
                    err.status(),
                    Some(StatusCode::BAD_REQUEST | StatusCode::METHOD_NOT_ALLOWED)
                ) => {}
            Err(err) => return Err(err),
        }

        let pab = BucketPublicAccessBlockConfiguration {
            block_public_acls: true,
            ignore_public_acls: true,
            block_public_policy: true,
            restrict_public_buckets: true,
        };

        match client
            .buckets()
            .put_public_access_block(&bucket)
            .configuration(pab)
            .send()
        {
            Ok(_) => {
                let got = client.buckets().get_public_access_block(&bucket).send()?;
                assert!(got.block_public_acls);
                match client.buckets().delete_public_access_block(&bucket).send() {
                    Ok(_) => {}
                    Err(err) if common::is_unsupported(&err) => {}
                    Err(err) => return Err(err),
                }
            }
            Err(err) if common::is_unsupported(&err) => {}
            Err(err)
                if matches!(
                    err.status(),
                    Some(StatusCode::BAD_REQUEST | StatusCode::METHOD_NOT_ALLOWED)
                ) => {}
            Err(err) => return Err(err),
        }

        Ok(())
    })
}

#[test]
fn s3_compat_blocking_list_v2_manual_pagination() -> Result<(), Error> {
    let Some(cfg) = common::load_config()? else {
        return Ok(());
    };

    let client = common::build_blocking_client(&cfg, AddressingStyle::Auto)?;
    common::with_bucket_blocking(&client, "s3-it-blocking-v2-", |bucket| {
        for key in ["a.txt", "b.txt", "c.txt"] {
            client
                .objects()
                .put(&bucket, key)
                .body_bytes(Bytes::from_static(b"x"))
                .send()?;
        }

        let mut keys = Vec::new();
        let mut token = None::<String>;
        loop {
            let mut req = client.objects().list_v2(&bucket).max_keys(1)?;
            if let Some(t) = token.take() {
                req = req.continuation_token(t)?;
            }
            let out = req.send()?;
            keys.extend(out.contents.iter().map(|o| o.key.clone()));
            if out.is_truncated {
                token = out.next_continuation_token.clone();
                assert!(token.is_some());
                continue;
            }
            break;
        }

        keys.sort();
        assert_eq!(keys, vec!["a.txt", "b.txt", "c.txt"]);
        Ok(())
    })
}

#[test]
fn s3_compat_blocking_presign_put_head_delete_roundtrip() -> Result<(), Error> {
    let Some(cfg) = common::load_config()? else {
        return Ok(());
    };

    let client = common::build_blocking_client(&cfg, AddressingStyle::Auto)?;
    common::with_bucket_blocking(&client, "s3-it-blocking-presign-", |bucket| {
        let key = "presigned.txt";
        let body = Bytes::from_static(b"presigned-body");

        let presigned_put = client
            .objects()
            .presign_put(&bucket, key)
            .header(
                http::header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain"),
            )?
            .metadata("m", "1")?
            .build()?;

        let mut req = ureq::agent().put(presigned_put.url.as_str());
        for (name, value) in presigned_put.headers.iter() {
            let Ok(value_str) = value.to_str() else {
                continue;
            };
            req = req.header(name.as_str(), value_str);
        }
        let resp = req
            .send(body.as_ref())
            .map_err(|e| Error::transport("presigned put failed", Some(Box::new(e))))?;
        assert!(resp.status().is_success());

        let head = client.objects().head(&bucket, key).send()?;
        assert_eq!(head.content_length, Some(body.len() as u64));
        assert_eq!(head.content_type.as_deref(), Some("text/plain"));

        let presigned_head = client.objects().presign_head(&bucket, key).build()?;
        let mut req = ureq::agent().head(presigned_head.url.as_str());
        for (name, value) in presigned_head.headers.iter() {
            let Ok(value_str) = value.to_str() else {
                continue;
            };
            req = req.header(name.as_str(), value_str);
        }
        let resp = req
            .call()
            .map_err(|e| Error::transport("presigned head failed", Some(Box::new(e))))?;
        assert!(resp.status().is_success());
        assert_eq!(
            resp.headers()
                .get("x-amz-meta-m")
                .and_then(|v| v.to_str().ok()),
            Some("1")
        );

        let presigned_delete = client.objects().presign_delete(&bucket, key).build()?;
        let mut req = ureq::agent().delete(presigned_delete.url.as_str());
        for (name, value) in presigned_delete.headers.iter() {
            let Ok(value_str) = value.to_str() else {
                continue;
            };
            req = req.header(name.as_str(), value_str);
        }
        let resp = req
            .call()
            .map_err(|e| Error::transport("presigned delete failed", Some(Box::new(e))))?;
        assert!(resp.status().is_success());

        match client.objects().head(&bucket, key).send() {
            Ok(_) => panic!("expected not found after delete"),
            Err(Error::Api(error)) => assert_eq!(error.status, StatusCode::NOT_FOUND),
            Err(other) => panic!("expected api error, got {other:?}"),
        }

        Ok(())
    })
}

#[cfg(feature = "multipart")]
#[test]
fn s3_compat_blocking_multipart_put_get_roundtrip() -> Result<(), Error> {
    let Some(cfg) = common::load_config()? else {
        return Ok(());
    };

    let client = common::build_blocking_client(&cfg, AddressingStyle::Auto)?;
    common::with_bucket_blocking(&client, "s3-it-blocking-mpu-", |bucket| {
        let key = "mpu.txt";
        let create = client
            .objects()
            .create_multipart_upload(&bucket, key)
            .send()?;
        let upload_id = create.upload_id;

        let part1 = Bytes::from(vec![b'a'; 5 * 1024 * 1024]);
        let part2 = Bytes::from_static(b"world");

        let p1 = client
            .objects()
            .upload_part(&bucket, key, &upload_id, 1)
            .body_bytes(part1.clone())
            .send()?;
        let p2 = client
            .objects()
            .upload_part(&bucket, key, &upload_id, 2)
            .body_bytes(part2.clone())
            .send()?;
        let etag1 = p1
            .etag
            .ok_or_else(|| Error::decode("missing upload part etag", None))?;
        let etag2 = p2
            .etag
            .ok_or_else(|| Error::decode("missing upload part etag", None))?;

        let parts = client
            .objects()
            .list_parts(&bucket, key, &upload_id)
            .max_parts(1)?
            .send()?;
        assert!(!parts.parts.is_empty());

        let marker = parts.parts.last().map(|p| p.part_number).unwrap_or(0);
        let mut saw_part2 = parts.parts.iter().any(|p| p.part_number == 2);

        let parts = client
            .objects()
            .list_parts(&bucket, key, &upload_id)
            .part_number_marker(marker)?
            .max_parts(1000)?
            .send()?;
        saw_part2 = saw_part2 || parts.parts.iter().any(|p| p.part_number == 2);

        if !saw_part2 {
            let parts = client
                .objects()
                .list_parts(&bucket, key, &upload_id)
                .part_number_marker(marker.saturating_add(1))?
                .max_parts(1000)?
                .send()?;
            saw_part2 = saw_part2 || parts.parts.iter().any(|p| p.part_number == 2);
        }

        if common::strict() {
            assert!(saw_part2);
        }

        client
            .objects()
            .complete_multipart_upload(&bucket, key, &upload_id)
            .part(2, etag2)?
            .part(1, etag1)?
            .send()?;

        let got = client.objects().get(&bucket, key).send()?.bytes()?;
        assert_eq!(got.len(), part1.len() + part2.len());
        let got = got.as_ref();
        assert!(got[..part1.len()].iter().all(|&b| b == b'a'));
        assert_eq!(&got[part1.len()..], part2.as_ref());

        client.objects().delete(&bucket, key).send()?;
        Ok(())
    })
}

#[cfg(feature = "multipart")]
#[test]
fn s3_compat_blocking_multipart_upload_part_copy_roundtrip() -> Result<(), Error> {
    let Some(cfg) = common::load_config()? else {
        return Ok(());
    };

    let client = common::build_blocking_client(&cfg, AddressingStyle::Auto)?;
    common::with_bucket_blocking(&client, "s3-it-blocking-mpu-copy-", |bucket| {
        let src_key = "src.bin";
        let dst_key = "dst.bin";
        let src = Bytes::from(vec![b'x'; 5 * 1024 * 1024]);
        client
            .objects()
            .put(&bucket, src_key)
            .body_bytes(src.clone())
            .send()?;

        let create = client
            .objects()
            .create_multipart_upload(&bucket, dst_key)
            .send()?;
        let upload_id = create.upload_id;

        let copied = client
            .objects()
            .upload_part_copy(&bucket, src_key, &bucket, dst_key, &upload_id, 1)
            .copy_source_range_bytes(0, src.len() as u64 - 1)?
            .send()?;
        let etag1 = copied
            .etag
            .ok_or_else(|| Error::decode("missing upload part copy etag", None))?;

        let tail = Bytes::from_static(b"tail");
        let p2 = client
            .objects()
            .upload_part(&bucket, dst_key, &upload_id, 2)
            .body_bytes(tail.clone())
            .send()?;
        let etag2 = p2
            .etag
            .ok_or_else(|| Error::decode("missing upload part etag", None))?;

        client
            .objects()
            .complete_multipart_upload(&bucket, dst_key, &upload_id)
            .part(1, etag1)?
            .part(2, etag2)?
            .send()?;

        let got = client.objects().get(&bucket, dst_key).send()?.bytes()?;
        assert_eq!(got.len(), src.len() + tail.len());
        assert_eq!(&got[..src.len()], src.as_ref());
        assert_eq!(&got[src.len()..], tail.as_ref());

        client.objects().delete(&bucket, src_key).send()?;
        client.objects().delete(&bucket, dst_key).send()?;
        Ok(())
    })
}

#[cfg(feature = "multipart")]
#[test]
fn s3_compat_blocking_multipart_abort_roundtrip() -> Result<(), Error> {
    let Some(cfg) = common::load_config()? else {
        return Ok(());
    };

    let client = common::build_blocking_client(&cfg, AddressingStyle::Auto)?;
    common::with_bucket_blocking(&client, "s3-it-blocking-mpu-abort-", |bucket| {
        let key = "abort.txt";
        let create = client
            .objects()
            .create_multipart_upload(&bucket, key)
            .send()?;
        let upload_id = create.upload_id;

        client
            .objects()
            .upload_part(&bucket, key, &upload_id, 1)
            .body_bytes(Bytes::from_static(b"tiny"))
            .send()?;

        client
            .objects()
            .abort_multipart_upload(&bucket, key, &upload_id)
            .send()?;

        match client.objects().list_parts(&bucket, key, &upload_id).send() {
            Ok(_) => panic!("expected error after abort"),
            Err(Error::Api(error)) => {
                let status = error.status;
                assert!(matches!(
                    status,
                    StatusCode::NOT_FOUND | StatusCode::BAD_REQUEST
                ));
            }
            Err(other) => panic!("expected api error, got {other:?}"),
        }

        Ok(())
    })
}
