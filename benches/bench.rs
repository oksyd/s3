#[cfg(any(feature = "_async", feature = "_blocking", feature = "checksums"))]
use std::{hint::black_box, time::Duration};

#[cfg(any(feature = "_async", feature = "_blocking", feature = "checksums"))]
use criterion::BenchmarkId;
#[cfg(feature = "checksums")]
use criterion::Throughput;
use criterion::{Criterion, criterion_group, criterion_main};

#[cfg(feature = "_async")]
fn async_client(addressing: s3::AddressingStyle) -> s3::Client {
    let creds = s3::Credentials::new("AKIDEXAMPLE", "SECRETKEYEXAMPLE")
        .expect("static credentials must be valid");

    s3::Client::builder("https://s3.example.com")
        .expect("endpoint must be valid")
        .region("us-east-1")
        .auth(s3::Auth::Static(creds))
        .addressing_style(addressing)
        .build()
        .expect("client must build")
}

#[cfg(feature = "_blocking")]
fn blocking_client(addressing: s3::AddressingStyle) -> s3::BlockingClient {
    let creds = s3::Credentials::new("AKIDEXAMPLE", "SECRETKEYEXAMPLE")
        .expect("static credentials must be valid");

    s3::BlockingClient::builder("https://s3.example.com")
        .expect("endpoint must be valid")
        .region("us-east-1")
        .auth(s3::Auth::Static(creds))
        .addressing_style(addressing)
        .build()
        .expect("client must build")
}

#[cfg(feature = "_async")]
fn bench_presign_async(c: &mut Criterion) {
    let client_path = async_client(s3::AddressingStyle::Path);
    let client_virtual = async_client(s3::AddressingStyle::VirtualHosted);
    let bucket = "my-bucket";

    let mut group = c.benchmark_group("presign_async");
    group.measurement_time(Duration::from_secs(3));

    for (label, client) in [("path", &client_path), ("virtual", &client_virtual)] {
        let key = "a/b/c/object.txt";
        group.bench_function(BenchmarkId::new("get_minimal", label), |b| {
            b.iter(|| {
                let req = client
                    .objects()
                    .presign_get(black_box(bucket), black_box(key))
                    .build()
                    .expect("presign must succeed");
                black_box(req);
            });
        });

        group.bench_function(BenchmarkId::new("put_with_headers", label), |b| {
            let ct = http::HeaderValue::from_static("application/octet-stream");
            b.iter(|| {
                let req = client
                    .objects()
                    .presign_put(black_box(bucket), black_box(key))
                    .expires_in(Duration::from_secs(60))
                    .expect("valid presign expiry")
                    .header(http::header::CONTENT_TYPE, ct.clone())
                    .expect("valid presign header")
                    .metadata("m1", "v1")
                    .expect("valid metadata")
                    .metadata("m2", "v2")
                    .expect("valid metadata")
                    .query_param("x-id", "PutObject")
                    .expect("valid query parameter")
                    .build()
                    .expect("presign must succeed");
                black_box(req);
            });
        });
    }

    group.finish();
}

#[cfg(not(feature = "_async"))]
fn bench_presign_async(c: &mut Criterion) {
    let _ = c;
}

#[cfg(feature = "_blocking")]
fn bench_presign_blocking(c: &mut Criterion) {
    let client_path = blocking_client(s3::AddressingStyle::Path);
    let client_virtual = blocking_client(s3::AddressingStyle::VirtualHosted);
    let bucket = "my-bucket";

    let mut group = c.benchmark_group("presign_blocking");
    group.measurement_time(Duration::from_secs(3));

    for (label, client) in [("path", &client_path), ("virtual", &client_virtual)] {
        let key = "a/b/c/object.txt";
        group.bench_function(BenchmarkId::new("get_minimal", label), |b| {
            b.iter(|| {
                let req = client
                    .objects()
                    .presign_get(black_box(bucket), black_box(key))
                    .build()
                    .expect("presign must succeed");
                black_box(req);
            });
        });

        group.bench_function(BenchmarkId::new("put_with_headers", label), |b| {
            let ct = http::HeaderValue::from_static("application/octet-stream");
            b.iter(|| {
                let req = client
                    .objects()
                    .presign_put(black_box(bucket), black_box(key))
                    .expires_in(Duration::from_secs(60))
                    .expect("valid presign expiry")
                    .header(http::header::CONTENT_TYPE, ct.clone())
                    .expect("valid presign header")
                    .metadata("m1", "v1")
                    .expect("valid metadata")
                    .metadata("m2", "v2")
                    .expect("valid metadata")
                    .query_param("x-id", "PutObject")
                    .expect("valid query parameter")
                    .build()
                    .expect("presign must succeed");
                black_box(req);
            });
        });
    }

    group.finish();
}

#[cfg(not(feature = "_blocking"))]
fn bench_presign_blocking(c: &mut Criterion) {
    let _ = c;
}

#[cfg(feature = "checksums")]
fn bench_checksums(c: &mut Criterion) {
    use s3::types::{Checksum, ChecksumAlgorithm};

    let mut group = c.benchmark_group("checksums");
    group.measurement_time(Duration::from_secs(3));

    let algorithms = [
        ("crc32", ChecksumAlgorithm::Crc32),
        ("crc32c", ChecksumAlgorithm::Crc32c),
        ("crc64nvme", ChecksumAlgorithm::Crc64Nvme),
    ];

    for size in [0usize, 32, 1024, 64 * 1024] {
        let bytes = vec![0xAB; size];
        group.throughput(Throughput::Bytes(size as u64));

        for (name, algorithm) in algorithms {
            group.bench_with_input(BenchmarkId::new(name, size), &bytes, |b, input| {
                b.iter(|| {
                    let checksum = Checksum::from_bytes(algorithm, black_box(input));
                    black_box(checksum);
                });
            });
        }
    }

    group.finish();
}

#[cfg(not(feature = "checksums"))]
fn bench_checksums(c: &mut Criterion) {
    let _ = c;
}

criterion_group!(
    benches,
    bench_presign_async,
    bench_presign_blocking,
    bench_checksums
);
criterion_main!(benches);
