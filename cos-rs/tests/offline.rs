use cos_rs::{BaseUrl, BucketGetOptions, Client, ObjectPutOptions, ServiceGetOptions};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;
use url::Url;

fn serve_once<F>(assert_request: F, body: &'static str) -> Url
where
    F: FnOnce(&str) + Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let request = read_request(&mut stream);
        assert_request(&request);
        let response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/xml\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        stream.write_all(response.as_bytes()).unwrap();
    });
    Url::parse(&format!("http://{addr}")).unwrap()
}

fn read_request(stream: &mut std::net::TcpStream) -> String {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        let n = stream.read(&mut tmp).unwrap();
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        if let Some(header_end) = find_header_end(&buf) {
            let headers = String::from_utf8_lossy(&buf[..header_end]).to_string();
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or(0);
            let body_start = header_end + 4;
            while buf.len() < body_start + content_length {
                let n = stream.read(&mut tmp).unwrap();
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&tmp[..n]);
            }
            break;
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

#[tokio::test]
async fn service_get_parses_xml_and_query() {
    let url = serve_once(
        |request| {
            assert!(
                request.starts_with("GET /?max-keys=10 HTTP/1.1"),
                "{request}"
            );
            assert!(request.contains("user-agent: cos-rs/"), "{request}");
        },
        r#"<ListAllMyBucketsResult><Owner><ID>uin</ID></Owner><Buckets><Bucket><Name>b-1</Name><Location>ap-guangzhou</Location></Bucket></Buckets></ListAllMyBucketsResult>"#,
    );
    let mut base = BaseUrl::new();
    base.service = Some(url);
    let client = Client::builder().base_url(base).build().unwrap();
    let (result, response) = client
        .service()
        .get(Some(ServiceGetOptions {
            max_keys: Some(10),
            ..Default::default()
        }))
        .await
        .unwrap();
    assert!(response.is_success());
    assert_eq!(result.buckets.buckets[0].name, "b-1");
}

#[tokio::test]
async fn bucket_get_sends_query_options() {
    let url = serve_once(
        |request| {
            assert!(
                request.starts_with("GET /?prefix=dir&max-keys=2 HTTP/1.1"),
                "{request}"
            );
        },
        r#"<ListBucketResult><Name>b-1</Name><MaxKeys>2</MaxKeys><Contents><Key>dir/a.txt</Key><Size>3</Size></Contents></ListBucketResult>"#,
    );
    let mut base = BaseUrl::new();
    base.bucket = Some(url);
    let client = Client::builder().base_url(base).build().unwrap();
    let (result, _) = client
        .bucket()
        .get(Some(BucketGetOptions {
            prefix: Some("dir".to_owned()),
            max_keys: Some(2),
            ..Default::default()
        }))
        .await
        .unwrap();
    assert_eq!(result.contents[0].key, "dir/a.txt");
}

#[tokio::test]
async fn object_put_encodes_key_headers_and_body() {
    let url = serve_once(
        |request| {
            assert!(
                request.starts_with("PUT /dir/hello%20world.txt HTTP/1.1"),
                "{request}"
            );
            assert!(request.contains("content-type: text/plain"), "{request}");
            assert!(request.ends_with("hello"), "{request}");
        },
        "",
    );
    let mut base = BaseUrl::new();
    base.bucket = Some(url);
    let client = Client::builder().base_url(base).build().unwrap();
    let response = client
        .object()
        .put(
            "dir/hello world.txt",
            "hello",
            Some(ObjectPutOptions {
                content_type: Some("text/plain".to_owned()),
                ..Default::default()
            }),
        )
        .await
        .unwrap();
    assert_eq!(response.status_code(), 200);
}
