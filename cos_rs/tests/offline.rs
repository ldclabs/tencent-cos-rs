use cos_rs::{
    AuthTime, BaseUrl, BucketGetObjectVersionsOptions, BucketGetOptions, BucketPutAclOptions,
    BucketVersioningConfiguration, Client, CompletePart, Config, CreateVectorBucketOptions,
    Credential, DeleteObject, DeleteVectorsOptions, Error, GetVectorsOptions, InputVector,
    ListVectorsOptions, ObjectDeleteOptions, ObjectOptionsOptions, ObjectPutOptions,
    PresignedUrlOptions, PutVectorsOptions, QueryVectorsOptions, RequestOptions, RetryOptions,
    ServiceGetOptions, VectorData, VectorEncryptionConfig,
};
use reqwest::Method;
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;
use url::Url;

struct MockResponse {
    status: u16,
    content_type: Option<&'static str>,
    headers: Vec<(&'static str, &'static str)>,
    body: &'static str,
}

impl MockResponse {
    fn xml(status: u16, body: &'static str) -> Self {
        Self {
            status,
            content_type: Some("application/xml"),
            headers: Vec::new(),
            body,
        }
    }

    fn json(status: u16, body: &'static str) -> Self {
        Self {
            status,
            content_type: Some("application/json"),
            headers: Vec::new(),
            body,
        }
    }

    fn empty(status: u16) -> Self {
        Self {
            status,
            content_type: None,
            headers: Vec::new(),
            body: "",
        }
    }

    fn header(mut self, key: &'static str, value: &'static str) -> Self {
        self.headers.push((key, value));
        self
    }
}

struct MockServer {
    url: Url,
    requests: Receiver<String>,
}

impl MockServer {
    fn url(&self) -> Url {
        self.url.clone()
    }

    fn next_request(&self) -> String {
        self.requests
            .recv_timeout(Duration::from_secs(5))
            .expect("mock server did not receive request")
    }
}

fn serve_once(body: &'static str) -> MockServer {
    serve_responses(vec![MockResponse::xml(200, body)])
}

fn serve_responses(responses: Vec<MockResponse>) -> MockServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        for response in responses {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            let _ = tx.send(request);
            write_response(&mut stream, response);
        }
    });
    MockServer {
        url: Url::parse(&format!("http://{addr}")).unwrap(),
        requests: rx,
    }
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

fn write_response(stream: &mut std::net::TcpStream, response: MockResponse) {
    let body_len = response.body.len();
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        reason_phrase(response.status),
        body_len
    );
    if let Some(content_type) = response.content_type {
        head.push_str(&format!("Content-Type: {content_type}\r\n"));
    }
    for (key, value) in response.headers {
        head.push_str(&format!("{key}: {value}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes()).unwrap();
    stream.write_all(response.body.as_bytes()).unwrap();
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        404 => "Not Found",
        409 => "Conflict",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n")
}

fn request_body(request: &str) -> &str {
    request.split("\r\n\r\n").nth(1).unwrap_or("")
}

fn request_json(request: &str) -> Value {
    serde_json::from_str(request_body(request)).unwrap()
}

fn assert_header(request: &str, name: &str, value: &str) {
    let found = request.lines().any(|line| {
        line.split_once(':')
            .map(|(header, actual)| header.eq_ignore_ascii_case(name) && actual.trim() == value)
            .unwrap_or(false)
    });
    assert!(found, "missing header {name}: {value}\n{request}");
}

fn assert_no_header(request: &str, name: &str) {
    let found = request.lines().any(|line| {
        line.split_once(':')
            .map(|(header, _)| header.eq_ignore_ascii_case(name))
            .unwrap_or(false)
    });
    assert!(!found, "unexpected header {name}\n{request}");
}

fn bucket_client(url: Url) -> Client {
    let mut base = BaseUrl::new();
    base.bucket = Some(url);
    Client::builder().base_url(base).build().unwrap()
}

fn vector_client(url: Url) -> Client {
    let mut base = BaseUrl::new();
    base.vector = Some(url);
    Client::builder().base_url(base).build().unwrap()
}

#[tokio::test]
async fn service_get_parses_xml_and_query() {
    let server = serve_once(
        r#"<ListAllMyBucketsResult><Owner><ID>uin</ID></Owner><Buckets><Bucket><Name>b-1</Name><Location>ap-guangzhou</Location></Bucket></Buckets></ListAllMyBucketsResult>"#,
    );
    let mut base = BaseUrl::new();
    base.service = Some(server.url());
    let client = Client::builder().base_url(base).build().unwrap();
    let (result, response) = client
        .service()
        .get(Some(ServiceGetOptions {
            max_keys: Some(10),
            ..Default::default()
        }))
        .await
        .unwrap();

    let request = server.next_request();
    assert!(
        request.starts_with("GET /?max-keys=10 HTTP/1.1"),
        "{request}"
    );
    assert_header(
        &request,
        "user-agent",
        &format!("cos-rs/{}", env!("CARGO_PKG_VERSION")),
    );
    assert!(response.is_success());
    assert_eq!(result.buckets.buckets[0].name, "b-1");
}

#[tokio::test]
async fn bucket_get_sends_query_options() {
    let server = serve_once(
        r#"<ListBucketResult><Name>b-1</Name><MaxKeys>2</MaxKeys><Contents><Key>dir/a.txt</Key><Size>3</Size></Contents></ListBucketResult>"#,
    );
    let client = bucket_client(server.url());
    let (result, _) = client
        .bucket()
        .get(Some(BucketGetOptions {
            prefix: Some("dir".to_owned()),
            max_keys: Some(2),
            ..Default::default()
        }))
        .await
        .unwrap();

    let request = server.next_request();
    assert!(
        request.starts_with("GET /?prefix=dir&max-keys=2 HTTP/1.1"),
        "{request}"
    );
    assert_eq!(result.contents[0].key, "dir/a.txt");
}

#[tokio::test]
async fn bucket_get_acl_parses_go_fixture() {
    let server = serve_once(
        r#"<AccessControlPolicy>
    <Owner>
        <ID>qcs::cam::uin/100000760461:uin/100000760461</ID>
        <DisplayName>qcs::cam::uin/100000760461:uin/100000760461</DisplayName>
    </Owner>
    <AccessControlList>
        <Grant>
            <Grantee xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="RootAccount">
                <ID>qcs::cam::uin/100000760461:uin/100000760461</ID>
                <DisplayName>qcs::cam::uin/100000760461:uin/100000760461</DisplayName>
            </Grantee>
            <Permission>FULL_CONTROL</Permission>
        </Grant>
        <Grant>
            <Grantee xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xsi:type="RootAccount">
                <URI>http://cam.qcloud.com/groups/global/AllUsers</URI>
            </Grantee>
            <Permission>READ</Permission>
        </Grant>
    </AccessControlList>
</AccessControlPolicy>"#,
    );
    let client = bucket_client(server.url());

    let (acl, _) = client.bucket().get_acl().await.unwrap();

    let request = server.next_request();
    assert!(request.starts_with("GET /?acl HTTP/1.1"), "{request}");
    assert_eq!(
        acl.owner.unwrap().id,
        "qcs::cam::uin/100000760461:uin/100000760461"
    );
    assert_eq!(acl.access_control_list.grants.len(), 2);
    assert_eq!(acl.access_control_list.grants[0].permission, "FULL_CONTROL");
    assert_eq!(
        acl.access_control_list.grants[1]
            .grantee
            .as_ref()
            .unwrap()
            .uri,
        "http://cam.qcloud.com/groups/global/AllUsers"
    );
}

#[tokio::test]
async fn bucket_put_acl_sends_header_options() {
    let server = serve_responses(vec![MockResponse::empty(200)]);
    let client = bucket_client(server.url());

    client
        .bucket()
        .put_acl(BucketPutAclOptions {
            x_cos_acl: Some("public-read".to_owned()),
            x_cos_grant_full_control: Some(
                "id=\"qcs::cam::uin/100000760461:uin/100000760461\"".to_owned(),
            ),
            ..Default::default()
        })
        .await
        .unwrap();

    let request = server.next_request();
    assert!(request.starts_with("PUT /?acl HTTP/1.1"), "{request}");
    assert_header(&request, "x-cos-acl", "public-read");
    assert_header(
        &request,
        "x-cos-grant-full-control",
        "id=\"qcs::cam::uin/100000760461:uin/100000760461\"",
    );
}

#[tokio::test]
async fn bucket_put_and_get_versioning_match_go_sdk() {
    let server = serve_responses(vec![
        MockResponse::empty(200),
        MockResponse::xml(
            200,
            r#"<VersioningConfiguration><Status>Suspended</Status></VersioningConfiguration>"#,
        ),
    ]);
    let client = bucket_client(server.url());

    client.bucket().put_versioning("Suspended").await.unwrap();
    let (versioning, _) = client.bucket().get_versioning().await.unwrap();

    let put_request = server.next_request();
    assert!(
        put_request.starts_with("PUT /?versioning HTTP/1.1"),
        "{put_request}"
    );
    assert_header(&put_request, "content-type", "application/xml");
    assert!(
        request_body(&put_request).contains(
            "<VersioningConfiguration><Status>Suspended</Status></VersioningConfiguration>"
        ),
        "{put_request}"
    );

    let get_request = server.next_request();
    assert!(
        get_request.starts_with("GET /?versioning HTTP/1.1"),
        "{get_request}"
    );
    assert_eq!(
        versioning,
        BucketVersioningConfiguration {
            status: "Suspended".to_owned()
        }
    );
}

#[tokio::test]
async fn bucket_get_object_versions_parses_repeated_entries() {
    let server = serve_once(
        r#"<ListVersionsResult>
  <Name>b-1</Name>
  <Prefix>put_opts</Prefix>
  <KeyMarker/>
  <VersionIdMarker/>
  <MaxKeys>100</MaxKeys>
  <IsTruncated>false</IsTruncated>
  <Version>
    <Key>put_opts</Key>
    <VersionId>v2</VersionId>
    <IsLatest>true</IsLatest>
    <LastModified>2026-07-01T08:00:01.000Z</LastModified>
    <ETag>&quot;e2&quot;</ETag>
    <Size>1</Size>
    <StorageClass>STANDARD</StorageClass>
  </Version>
  <Version>
    <Key>put_opts</Key>
    <VersionId>v1</VersionId>
    <IsLatest>false</IsLatest>
    <LastModified>2026-07-01T08:00:00.000Z</LastModified>
    <ETag>&quot;e1&quot;</ETag>
    <Size>1</Size>
    <StorageClass>STANDARD</StorageClass>
  </Version>
  <DeleteMarker>
    <Key>deleted</Key>
    <VersionId>d1</VersionId>
    <IsLatest>true</IsLatest>
    <LastModified>2026-07-01T08:00:02.000Z</LastModified>
  </DeleteMarker>
</ListVersionsResult>"#,
    );
    let client = bucket_client(server.url());

    let (result, _) = client
        .bucket()
        .get_object_versions(Some(BucketGetObjectVersionsOptions {
            prefix: Some("put_opts".to_owned()),
            max_keys: Some(100),
            ..Default::default()
        }))
        .await
        .unwrap();

    let request = server.next_request();
    assert!(
        request.starts_with("GET /?versions&prefix=put_opts&max-keys=100 HTTP/1.1"),
        "{request}"
    );
    assert_eq!(result.name, "b-1");
    assert_eq!(result.versions.len(), 2);
    assert_eq!(result.versions[0].version_id, "v2");
    assert_eq!(result.versions[1].version_id, "v1");
    assert_eq!(result.delete_markers.len(), 1);
    assert_eq!(result.delete_markers[0].version_id, "d1");
}

#[tokio::test]
async fn bucket_get_location_parses_text_result() {
    let server = serve_once(
        "<?xml version='1.0' encoding='utf-8' ?><LocationConstraint>ap-guangzhou</LocationConstraint>",
    );
    let client = bucket_client(server.url());

    let (location, _) = client.bucket().get_location().await.unwrap();

    let request = server.next_request();
    assert!(request.starts_with("GET /?location HTTP/1.1"), "{request}");
    assert_eq!(location.location, "ap-guangzhou");
}

#[tokio::test]
async fn object_put_encodes_key_headers_and_body() {
    let server = serve_responses(vec![MockResponse::empty(200)]);
    let client = bucket_client(server.url());
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

    let request = server.next_request();
    assert!(
        request.starts_with("PUT /dir/hello%20world.txt HTTP/1.1"),
        "{request}"
    );
    assert_header(&request, "content-type", "text/plain");
    assert_eq!(request_body(&request), "hello");
    assert_eq!(response.status_code(), 200);
}

#[tokio::test]
async fn object_delete_options_and_existence_match_go_sdk() {
    let server = serve_responses(vec![
        MockResponse::empty(204),
        MockResponse::xml(
            404,
            r#"<Error><Code>NoSuchKey</Code><Message>The specified key does not exist.</Message></Error>"#,
        ),
    ]);
    let client = bucket_client(server.url());

    client
        .object()
        .delete(
            "test/hello.txt",
            Some(ObjectDeleteOptions {
                version_id: Some("versionid".to_owned()),
            }),
        )
        .await
        .unwrap();
    let exists = client.object().is_exist("missing.txt").await.unwrap();

    let delete_request = server.next_request();
    assert!(
        delete_request.starts_with("DELETE /test/hello.txt?versionId=versionid HTTP/1.1"),
        "{delete_request}"
    );
    let head_request = server.next_request();
    assert!(
        head_request.starts_with("HEAD /missing.txt HTTP/1.1"),
        "{head_request}"
    );
    assert!(!exists);
}

#[tokio::test]
async fn object_options_sends_cors_preflight_headers() {
    let server = serve_responses(vec![MockResponse::empty(200)]);
    let client = bucket_client(server.url());

    client
        .object()
        .options(
            "test/hello.txt",
            ObjectOptionsOptions {
                origin: Some("www.qq.com".to_owned()),
                access_control_request_method: Some("PUT".to_owned()),
                access_control_request_headers: Some("x-cos-meta-test".to_owned()),
            },
        )
        .await
        .unwrap();

    let request = server.next_request();
    assert!(
        request.starts_with("OPTIONS /test/hello.txt HTTP/1.1"),
        "{request}"
    );
    assert_header(&request, "origin", "www.qq.com");
    assert_header(&request, "access-control-request-method", "PUT");
    assert_header(
        &request,
        "access-control-request-headers",
        "x-cos-meta-test",
    );
}

#[tokio::test]
async fn object_delete_multi_sends_xml_and_parses_result() {
    let server = serve_once(
        r#"<DeleteResult>
    <Deleted><Key>test1</Key></Deleted>
    <Deleted><Key>test3</Key></Deleted>
    <Deleted><Key>test2</Key></Deleted>
</DeleteResult>"#,
    );
    let client = bucket_client(server.url());

    let (result, _) = client
        .object()
        .delete_multi(
            vec![
                DeleteObject {
                    key: "test1".to_owned(),
                    version_id: None,
                },
                DeleteObject {
                    key: "test3".to_owned(),
                    version_id: None,
                },
                DeleteObject {
                    key: "test2".to_owned(),
                    version_id: None,
                },
            ],
            false,
        )
        .await
        .unwrap();

    let request = server.next_request();
    assert!(request.starts_with("POST /?delete HTTP/1.1"), "{request}");
    assert_header(&request, "content-type", "application/xml");
    assert!(request_body(&request).contains("<Quiet>false</Quiet>"));
    assert!(request_body(&request).contains("<Key>test1</Key>"));
    assert_eq!(result.deleted.len(), 3);
    assert_eq!(result.deleted[1].key, "test3");
}

#[tokio::test]
async fn object_multipart_lifecycle_requests_match_go_sdk() {
    let server = serve_responses(vec![
        MockResponse::xml(
            200,
            r#"<InitiateMultipartUploadResult><Bucket>b</Bucket><Key>test/hello.txt</Key><UploadId>upload-id</UploadId></InitiateMultipartUploadResult>"#,
        ),
        MockResponse::empty(200),
        MockResponse::xml(
            200,
            r#"<CompleteMultipartUploadResult><Location>http://example/test/hello.txt</Location><Bucket>b</Bucket><Key>test/hello.txt</Key><ETag>"etag"</ETag></CompleteMultipartUploadResult>"#,
        ),
        MockResponse::empty(204),
    ]);
    let client = bucket_client(server.url());

    let (init, _) = client
        .object()
        .initiate_multipart_upload("test/hello.txt", None)
        .await
        .unwrap();
    client
        .object()
        .upload_part("test/hello.txt", "upload-id", 1, "part-body")
        .await
        .unwrap();
    let (complete, _) = client
        .object()
        .complete_multipart_upload(
            "test/hello.txt",
            "upload-id",
            vec![CompletePart {
                part_number: 1,
                etag: "\"etag\"".to_owned(),
            }],
        )
        .await
        .unwrap();
    client
        .object()
        .abort_multipart_upload("test/hello.txt", "upload-id")
        .await
        .unwrap();

    assert_eq!(init.upload_id, "upload-id");
    assert_eq!(complete.etag, "\"etag\"");
    assert!(
        server
            .next_request()
            .starts_with("POST /test/hello.txt?uploads HTTP/1.1")
    );
    let upload_part = server.next_request();
    assert!(
        upload_part.starts_with("PUT /test/hello.txt?partNumber=1&uploadId=upload-id HTTP/1.1"),
        "{upload_part}"
    );
    assert_eq!(request_body(&upload_part), "part-body");
    let complete_request = server.next_request();
    assert!(
        complete_request.starts_with("POST /test/hello.txt?uploadId=upload-id HTTP/1.1"),
        "{complete_request}"
    );
    assert!(request_body(&complete_request).contains("<PartNumber>1</PartNumber>"));
    assert!(
        server
            .next_request()
            .starts_with("DELETE /test/hello.txt?uploadId=upload-id HTTP/1.1")
    );
}

#[test]
fn object_url_and_presigned_url_match_go_sdk_shape() {
    let mut base = BaseUrl::new();
    base.bucket =
        Some(Url::parse("http://examplebucket-1250000000.cos.ap-guangzhou.myqcloud.com").unwrap());
    let client = Client::builder().base_url(base).build().unwrap();

    let object_url = client
        .object()
        .get_object_url("dir/hello world.txt")
        .unwrap();
    assert_eq!(
        object_url.as_str(),
        "http://examplebucket-1250000000.cos.ap-guangzhou.myqcloud.com/dir/hello%20world.txt"
    );

    let credential = Credential::new("QmFzZTY0IGlzIGEgZ*******", "ZfbOA78asKUYBcXFrJD0a1I*******");
    let auth_time = AuthTime::fixed(1_622_702_557, 1_622_706_157);
    let presigned = client
        .object()
        .get_presigned_url(
            Method::PUT,
            "test.jpg",
            &credential,
            Duration::from_secs(3600),
            Some(PresignedUrlOptions {
                request_options: RequestOptions::new(),
                auth_time: Some(auth_time.clone()),
                sign_host: false,
                sign_merged: false,
            }),
        )
        .unwrap();
    let query = presigned.query().unwrap();
    assert!(query.contains("q-sign-algorithm=sha1"));
    assert!(query.contains("q-sign-time=1622702557%3B1622706157"));
    assert!(query.contains("q-key-time=1622702557%3B1622706157"));
    assert!(query.contains("q-header-list="));
    assert!(query.contains("q-url-param-list="));
    assert!(query.contains("q-signature=820975b5a8eccce9455b94d4ebed14d66654bf3c"));

    let merged = client
        .object()
        .get_presigned_url(
            Method::PUT,
            "test.jpg",
            &credential,
            Duration::from_secs(3600),
            Some(PresignedUrlOptions {
                request_options: RequestOptions::new().query("test", "params"),
                auth_time: Some(auth_time),
                sign_host: false,
                sign_merged: true,
            }),
        )
        .unwrap();
    let pairs = merged.query_pairs().collect::<BTreeMap<_, _>>();
    assert_eq!(pairs.get("test").unwrap(), "params");
    let sign = pairs.get("sign").expect("merged signature query");
    assert!(sign.starts_with("q-sign-algorithm=sha1&q-ak=QmFzZTY0"));
    assert!(sign.contains("q-url-param-list=test"));
    assert!(sign.contains("q-signature=7757e84ed5f8953eafc30afcd2a5d1ad68e00d67"));
}

#[tokio::test]
async fn cos_xml_api_error_parses_body() {
    let server = serve_responses(vec![MockResponse::xml(
        409,
        r#"<Error>
    <Code>BucketAlreadyExists</Code>
    <Message>The requested bucket name is not available.</Message>
    <Resource>testdelete-1253846586.cos.ap-guangzhou.myqcloud.com</Resource>
    <RequestId>NTk0NTRjZjZfNTViMjM1XzlkMV9hZTZh</RequestId>
    <TraceId>trace-id</TraceId>
</Error>"#,
    )]);
    let mut base = BaseUrl::new();
    base.service = Some(server.url());
    let client = Client::builder().base_url(base).build().unwrap();

    let err = client.service().get(None).await.unwrap_err();

    let request = server.next_request();
    assert!(request.starts_with("GET / HTTP/1.1"), "{request}");
    match err {
        Error::Api(resp) => {
            assert_eq!(resp.status.as_u16(), 409);
            assert_eq!(resp.code, "BucketAlreadyExists");
            assert_eq!(resp.request_id, "NTk0NTRjZjZfNTViMjM1XzlkMV9hZTZh");
        }
        other => panic!("expected COS API error, got {other:?}"),
    }
}

#[tokio::test]
async fn vector_create_bucket_sends_json_and_parses_response() {
    let server = serve_responses(vec![MockResponse::json(
        200,
        r#"{"vectorBucketQcs":"qcs::vector"}"#,
    )]);
    let client = vector_client(server.url());

    let (result, _) = client
        .vector()
        .create_vector_bucket(&CreateVectorBucketOptions {
            vector_bucket_name: "examplebucket-1250000000".to_owned(),
            encryption_configuration: Some(VectorEncryptionConfig {
                sse_type: "AES256".to_owned(),
            }),
        })
        .await
        .unwrap();

    let request = server.next_request();
    assert!(
        request.starts_with("POST /CreateVectorBucket HTTP/1.1"),
        "{request}"
    );
    assert_header(&request, "content-type", "application/json");
    let body = request_json(&request);
    assert_eq!(body["vectorBucketName"], "examplebucket-1250000000");
    assert_eq!(body["encryptionConfiguration"]["sseType"], "AES256");
    assert_eq!(result.vector_bucket_qcs, "qcs::vector");
}

#[tokio::test]
async fn vector_put_get_and_list_vectors_match_go_sdk_payloads() {
    let server = serve_responses(vec![
        MockResponse::empty(200),
        MockResponse::json(
            200,
            r#"{
                "vectors": [
                    {
                        "key": "doc-001",
                        "data": {"float32": [1.0, 2.0]},
                        "metadata": {"color": "red", "count": 10}
                    }
                ]
            }"#,
        ),
        MockResponse::json(200, r#"{"vectors":[{"key":"doc-001"}],"nextToken":"abc"}"#),
    ]);
    let client = vector_client(server.url());

    let mut metadata = BTreeMap::new();
    metadata.insert("title".to_owned(), Value::String("doc title".to_owned()));
    metadata.insert("category".to_owned(), Value::String("AI".to_owned()));
    client
        .vector()
        .put_vectors(
            &PutVectorsOptions {
                vector_bucket_name: "examplebucket-1250000000".to_owned(),
                index_name: "test-index".to_owned(),
            },
            vec![InputVector {
                key: "doc-001".to_owned(),
                data: VectorData {
                    float32: vec![0.1, 0.2, 0.3, 0.4],
                },
                metadata,
            }],
        )
        .await
        .unwrap();
    let (vectors, _) = client
        .vector()
        .get_vectors(
            &GetVectorsOptions {
                vector_bucket_name: "examplebucket-1250000000".to_owned(),
                index_name: "test-index".to_owned(),
                return_data: Some(true),
                return_metadata: Some(true),
            },
            vec!["doc-001".to_owned()],
        )
        .await
        .unwrap();
    let (listed, _) = client
        .vector()
        .list_vectors(&ListVectorsOptions {
            vector_bucket_name: "examplebucket-1250000000".to_owned(),
            index_name: "test-index".to_owned(),
            max_results: 10,
            next_token: String::new(),
            return_data: None,
            return_metadata: None,
            segment_count: 4,
            segment_index: 0,
        })
        .await
        .unwrap();

    let put_request = server.next_request();
    assert!(put_request.starts_with("POST /PutVectors HTTP/1.1"));
    let put_body = request_json(&put_request);
    assert_eq!(put_body["vectors"][0]["key"], "doc-001");
    assert_eq!(put_body["vectors"][0]["metadata"]["category"], "AI");

    let get_request = server.next_request();
    assert!(get_request.starts_with("POST /GetVectors HTTP/1.1"));
    let get_body = request_json(&get_request);
    assert_eq!(get_body["keys"][0], "doc-001");
    assert_eq!(get_body["returnData"], true);
    assert_eq!(
        vectors.vectors[0].data.as_ref().unwrap().float32,
        vec![1.0, 2.0]
    );

    let list_request = server.next_request();
    assert!(list_request.starts_with("POST /ListVectors HTTP/1.1"));
    let list_body = request_json(&list_request);
    assert_eq!(list_body["segmentCount"], 4);
    assert_eq!(list_body["segmentIndex"], 0);
    assert_eq!(listed.next_token, "abc");
}

#[tokio::test]
async fn vector_client_side_validation_matches_go_sdk() {
    let client = vector_client(Url::parse("http://127.0.0.1:9").unwrap());
    let list_base = ListVectorsOptions {
        vector_bucket_name: "examplebucket-1250000000".to_owned(),
        index_name: "test-index".to_owned(),
        max_results: 0,
        next_token: String::new(),
        return_data: None,
        return_metadata: None,
        segment_count: 0,
        segment_index: 1,
    };
    let err = client.vector().list_vectors(&list_base).await.unwrap_err();
    assert!(
        err.to_string()
            .contains("segmentIndex requires segmentCount")
    );

    let mut out_of_range = list_base.clone();
    out_of_range.segment_count = 17;
    out_of_range.segment_index = 0;
    let err = client
        .vector()
        .list_vectors(&out_of_range)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("segmentCount must be in [1,16]"));

    let err = client
        .vector()
        .put_vectors(
            &PutVectorsOptions {
                vector_bucket_name: "examplebucket-1250000000".to_owned(),
                index_name: "test-index".to_owned(),
            },
            Vec::new(),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("vectors param is empty"));

    let err = client
        .vector()
        .delete_vectors(
            &DeleteVectorsOptions {
                vector_bucket_name: "examplebucket-1250000000".to_owned(),
                index_name: "test-index".to_owned(),
            },
            Vec::new(),
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("keys is empty"));

    let err = client
        .vector()
        .query_vectors(
            &QueryVectorsOptions {
                vector_bucket_name: "examplebucket-1250000000".to_owned(),
                index_name: "test-index".to_owned(),
                filter: None,
                return_data: None,
                return_metadata: None,
                return_distance: None,
            },
            VectorData { float32: vec![0.1] },
            0,
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("topK must be greater than 0"));
}

#[tokio::test]
async fn vector_error_response_parses_validation_exception() {
    let server = serve_responses(vec![
        MockResponse::json(
            400,
            r#"{
                "message": "VectorBucketName is invalid",
                "fieldList": [
                    {
                        "message": "VectorBucketName should match pattern",
                        "path": "/vectorBucketName"
                    }
                ]
            }"#,
        )
        .header("X-Cos-Error-Code", "ValidationException")
        .header("X-Cos-Request-Id", "NjM3ZmI5YTlfOTBm"),
    ]);
    let client = vector_client(server.url());

    let err = client
        .vector()
        .create_vector_bucket(&CreateVectorBucketOptions {
            vector_bucket_name: "invalid".to_owned(),
            encryption_configuration: None,
        })
        .await
        .unwrap_err();

    let request = server.next_request();
    assert!(
        request.starts_with("POST /CreateVectorBucket HTTP/1.1"),
        "{request}"
    );
    match err {
        Error::Vector(resp) => {
            assert_eq!(resp.status.as_u16(), 400);
            assert_eq!(resp.code, "ValidationException");
            assert_eq!(resp.message, "VectorBucketName is invalid");
            assert_eq!(resp.request_id, "NjM3ZmI5YTlfOTBm");
            assert_eq!(resp.field_list[0].path, "/vectorBucketName");
        }
        other => panic!("expected vector error, got {other:?}"),
    }
}

#[tokio::test]
async fn vector_retries_500_and_marks_retry_attempt() {
    let server = serve_responses(vec![
        MockResponse::json(500, r#"{"message":"Internal server error"}"#)
            .header("X-Cos-Error-Code", "InternalServerException"),
        MockResponse::json(200, r#"{"vectors":[]}"#),
    ]);
    let mut base = BaseUrl::new();
    base.vector = Some(server.url());
    let client = Client::builder()
        .base_url(base)
        .config(Config {
            retry: RetryOptions {
                count: 2,
                interval: Duration::ZERO,
                auto_switch_host: false,
            },
            ..Default::default()
        })
        .build()
        .unwrap();

    let (result, _) = client
        .vector()
        .list_vectors(&ListVectorsOptions {
            vector_bucket_name: "examplebucket-1250000000".to_owned(),
            index_name: "test-index".to_owned(),
            max_results: 0,
            next_token: String::new(),
            return_data: None,
            return_metadata: None,
            segment_count: 0,
            segment_index: 0,
        })
        .await
        .unwrap();

    let first = server.next_request();
    let second = server.next_request();
    assert_no_header(&first, "x-cos-sdk-retry");
    assert_header(&second, "x-cos-sdk-retry", "true");
    assert!(result.vectors.is_empty());
}
