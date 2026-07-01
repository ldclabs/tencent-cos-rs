//! Client-side encryption support.
//!
//! Implemented pieces include the `MasterCipher` abstraction, a local AES-CTR
//! master cipher for tests/local use, a Tencent KMS master cipher, crypto
//! object put/get/delete, and a minimal TencentCloud TC3 KMS client used by
//! `KmsMasterCipher`.

use crate::encoding::insert_header;
use crate::error::{Error, Result};
use crate::object::{ObjectGetOptions, ObjectPutOptions};
use crate::{Client, Credential, Response};
use aes::Aes256;
use async_trait::async_trait;
use base64::Engine;
use bytes::Bytes;
use ctr::cipher::{KeyIvInit, StreamCipher};
use rand::Rng;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::Arc;

type Aes256Ctr = ctr::Ctr128BE<Aes256>;

pub const ENCRYPTION_UA_SUFFIX: &str = "coscrypto";
pub const AES_CTR_ALGORITHM: &str = "AES/CTR/NoPadding";
pub const COS_KMS_CRYPTO_WRAP: &str = "KMS/TencentCloud";
pub const COS_CLIENT_SIDE_ENCRYPTION_KEY: &str = "x-cos-meta-client-side-encryption-key";
pub const COS_CLIENT_SIDE_ENCRYPTION_START: &str = "x-cos-meta-client-side-encryption-start";
pub const COS_CLIENT_SIDE_ENCRYPTION_CEK_ALG: &str = "x-cos-meta-client-side-encryption-cek-alg";
pub const COS_CLIENT_SIDE_ENCRYPTION_WRAP_ALG: &str = "x-cos-meta-client-side-encryption-wrap-alg";
pub const COS_CLIENT_SIDE_ENCRYPTION_MAT_DESC: &str = "x-cos-meta-client-side-encryption-matdesc";
pub const COS_CLIENT_SIDE_ENCRYPTION_UNENCRYPTED_CONTENT_LENGTH: &str =
    "x-cos-meta-client-side-encryption-unencrypted-content-length";
pub const COS_CLIENT_SIDE_ENCRYPTION_UNENCRYPTED_CONTENT_MD5: &str =
    "x-cos-meta-client-side-encryption-unencrypted-content-md5";

#[async_trait]
/// Master key abstraction used to wrap per-object content keys and IVs.
pub trait MasterCipher: Clone + Send + Sync + 'static {
    /// Encrypt a content key or IV.
    async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>>;
    /// Decrypt a content key or IV.
    async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>>;
    /// Name written to COS encryption metadata.
    fn wrap_algorithm(&self) -> &str;
    /// Material description written to COS encryption metadata.
    fn material_description(&self) -> &str;
}

#[derive(Debug, Clone)]
/// Local AES-CTR master cipher.
///
/// This is useful for tests and local deployments. Production Tencent COS
/// client-side encryption usually uses [`KmsMasterCipher`].
pub struct LocalMasterCipher {
    key: Arc<[u8; 32]>,
    material_description: String,
}

impl LocalMasterCipher {
    pub fn new(key: [u8; 32], material_description: impl Into<String>) -> Self {
        Self {
            key: Arc::new(key),
            material_description: material_description.into(),
        }
    }
}

#[async_trait]
impl MasterCipher for LocalMasterCipher {
    async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let mut iv = [0u8; 16];
        rand::rng().fill_bytes(&mut iv);
        let mut buf = plaintext.to_vec();
        let mut cipher = aes256_ctr(self.key.as_ref(), &iv)?;
        cipher.apply_keystream(&mut buf);
        let mut out = iv.to_vec();
        out.extend(buf);
        Ok(out)
    }

    async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        if ciphertext.len() < 16 {
            return Err(Error::Crypto("encrypted key is too short".to_owned()));
        }
        let (iv, data) = ciphertext.split_at(16);
        let mut buf = data.to_vec();
        let mut cipher = aes256_ctr(self.key.as_ref(), iv)?;
        cipher.apply_keystream(&mut buf);
        Ok(buf)
    }

    fn wrap_algorithm(&self) -> &str {
        "AES/CTR"
    }

    fn material_description(&self) -> &str {
        &self.material_description
    }
}

#[derive(Clone)]
/// Tencent KMS-backed master cipher.
pub struct KmsMasterCipher {
    client: KmsClient,
    kms_id: String,
    material_description: String,
}

impl KmsMasterCipher {
    pub fn new(
        client: KmsClient,
        kms_id: impl Into<String>,
        desc: BTreeMap<String, String>,
    ) -> Result<Self> {
        let kms_id = kms_id.into();
        if kms_id.is_empty() {
            return Err(Error::Crypto("KMS ID is empty".to_owned()));
        }
        Ok(Self {
            client,
            kms_id,
            material_description: serde_json::to_string(&desc)?,
        })
    }
}

#[async_trait]
impl MasterCipher for KmsMasterCipher {
    async fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        self.client
            .encrypt(&self.kms_id, &self.material_description, plaintext)
            .await
    }

    async fn decrypt(&self, ciphertext: &[u8]) -> Result<Vec<u8>> {
        self.client
            .decrypt(&self.material_description, ciphertext)
            .await
    }

    fn wrap_algorithm(&self) -> &str {
        COS_KMS_CRYPTO_WRAP
    }

    fn material_description(&self) -> &str {
        &self.material_description
    }
}

#[derive(Clone)]
/// COS client wrapper exposing encrypted object operations.
pub struct CryptoClient<M: MasterCipher> {
    client: Client,
    object: CryptoObjectService<M>,
}

impl<M: MasterCipher> CryptoClient<M> {
    pub fn new(client: Client, master: M) -> Self {
        Self {
            client: client.clone(),
            object: CryptoObjectService { client, master },
        }
    }

    pub fn object(&self) -> &CryptoObjectService<M> {
        &self.object
    }

    pub fn inner(&self) -> &Client {
        &self.client
    }
}

#[derive(Clone)]
/// Encrypted Object API entry point.
pub struct CryptoObjectService<M: MasterCipher> {
    client: Client,
    master: M,
}

impl<M: MasterCipher> CryptoObjectService<M> {
    /// Encrypt and upload an object.
    pub async fn put(
        &self,
        key: &str,
        body: impl Into<Bytes>,
        options: Option<ObjectPutOptions>,
    ) -> Result<Response> {
        let plaintext = body.into();
        let mut content_key = [0u8; 32];
        let mut iv = [0u8; 16];
        rand::rng().fill_bytes(&mut content_key);
        rand::rng().fill_bytes(&mut iv);
        let mut encrypted = plaintext.to_vec();
        let mut cipher = aes256_ctr(&content_key, &iv)?;
        cipher.apply_keystream(&mut encrypted);

        let encrypted_key = self.master.encrypt(&content_key).await?;
        let encrypted_iv = self.master.encrypt(&iv).await?;
        let mut options = options.unwrap_or_default();
        add_crypto_headers(
            &mut options.extra_headers,
            self.master.wrap_algorithm(),
            self.master.material_description(),
            &encrypted_key,
            &encrypted_iv,
        )?;
        insert_header(
            &mut options.extra_headers,
            COS_CLIENT_SIDE_ENCRYPTION_UNENCRYPTED_CONTENT_LENGTH,
            &plaintext.len().to_string(),
        )?;
        self.client
            .object()
            .put(key, encrypted, Some(options))
            .await
    }

    /// Download and decrypt an encrypted object.
    pub async fn get(&self, key: &str, options: Option<ObjectGetOptions>) -> Result<Response> {
        let head = self.client.object().head(key, None).await?;
        let Some(envelope) = Envelope::from_headers(&head.headers)? else {
            return self.client.object().get(key, options).await;
        };
        if envelope.material_description != self.master.material_description() {
            return Err(Error::Crypto(format!(
                "provided master cipher error, want:{}, return:{}",
                self.master.material_description(),
                envelope.material_description
            )));
        }
        let content_key = self.master.decrypt(&envelope.encrypted_key).await?;
        let iv = self.master.decrypt(&envelope.encrypted_iv).await?;
        let mut response = self.client.object().get(key, options).await?;
        let mut decrypted = response.body.to_vec();
        let mut cipher = aes256_ctr(&content_key, &iv)?;
        cipher.apply_keystream(&mut decrypted);
        response.body = Bytes::from(decrypted);
        Ok(response)
    }

    /// Delete an object through the underlying Object API.
    pub async fn delete(&self, key: &str) -> Result<Response> {
        self.client.object().delete(key, None).await
    }
}

fn add_crypto_headers(
    headers: &mut HeaderMap,
    wrap_algorithm: &str,
    material_description: &str,
    encrypted_key: &[u8],
    encrypted_iv: &[u8],
) -> Result<()> {
    let b64 = base64::engine::general_purpose::STANDARD;
    insert_header(
        headers,
        COS_CLIENT_SIDE_ENCRYPTION_KEY,
        &b64.encode(encrypted_key),
    )?;
    insert_header(
        headers,
        COS_CLIENT_SIDE_ENCRYPTION_START,
        &b64.encode(encrypted_iv),
    )?;
    insert_header(headers, COS_CLIENT_SIDE_ENCRYPTION_WRAP_ALG, wrap_algorithm)?;
    insert_header(
        headers,
        COS_CLIENT_SIDE_ENCRYPTION_CEK_ALG,
        AES_CTR_ALGORITHM,
    )?;
    if !material_description.is_empty() {
        insert_header(
            headers,
            COS_CLIENT_SIDE_ENCRYPTION_MAT_DESC,
            material_description,
        )?;
    }
    Ok(())
}

fn aes256_ctr(key: &[u8], iv: &[u8]) -> Result<Aes256Ctr> {
    Aes256Ctr::new_from_slices(key, iv)
        .map_err(|e| Error::Crypto(format!("invalid AES-CTR key/iv length: {e:?}")))
}

#[derive(Debug)]
struct Envelope {
    encrypted_key: Vec<u8>,
    encrypted_iv: Vec<u8>,
    material_description: String,
}

impl Envelope {
    fn from_headers(headers: &HeaderMap) -> Result<Option<Self>> {
        let Some(key) = headers.get(COS_CLIENT_SIDE_ENCRYPTION_KEY) else {
            return Ok(None);
        };
        let b64 = base64::engine::general_purpose::STANDARD;
        let key = b64
            .decode(key.to_str().map_err(|e| Error::Crypto(e.to_string()))?)
            .map_err(|e| Error::Crypto(e.to_string()))?;
        let iv = headers
            .get(COS_CLIENT_SIDE_ENCRYPTION_START)
            .ok_or_else(|| Error::Crypto("missing encrypted IV".to_owned()))?;
        let iv = b64
            .decode(iv.to_str().map_err(|e| Error::Crypto(e.to_string()))?)
            .map_err(|e| Error::Crypto(e.to_string()))?;
        let material_description = headers
            .get(COS_CLIENT_SIDE_ENCRYPTION_MAT_DESC)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_owned();
        Ok(Some(Self {
            encrypted_key: key,
            encrypted_iv: iv,
            material_description,
        }))
    }
}

#[derive(Clone)]
/// Minimal TencentCloud KMS client for Encrypt/Decrypt.
pub struct KmsClient {
    http: reqwest::Client,
    credential: Credential,
    region: String,
    endpoint: String,
}

impl KmsClient {
    pub fn new(credential: Credential, region: impl Into<String>) -> Result<Self> {
        Ok(Self {
            http: reqwest::Client::new(),
            credential,
            region: region.into(),
            endpoint: "kms.tencentcloudapi.com".to_owned(),
        })
    }

    pub fn endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = endpoint.into();
        self
    }

    async fn encrypt(&self, key_id: &str, context: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
        let payload = serde_json::json!({
            "KeyId": key_id,
            "EncryptionContext": context,
            "Plaintext": base64::engine::general_purpose::STANDARD.encode(plaintext)
        });
        let response: KmsEncryptResponse = self.send("Encrypt", payload).await?;
        let blob = response.response.ciphertext_blob.ok_or_else(|| {
            Error::Crypto("KMS Encrypt response missing CiphertextBlob".to_owned())
        })?;
        Ok(blob.into_bytes())
    }

    async fn decrypt(&self, context: &str, ciphertext: &[u8]) -> Result<Vec<u8>> {
        let payload = serde_json::json!({
            "CiphertextBlob": String::from_utf8_lossy(ciphertext),
            "EncryptionContext": context
        });
        let response: KmsDecryptResponse = self.send("Decrypt", payload).await?;
        let plaintext = response
            .response
            .plaintext
            .ok_or_else(|| Error::Crypto("KMS Decrypt response missing Plaintext".to_owned()))?;
        base64::engine::general_purpose::STANDARD
            .decode(plaintext)
            .map_err(|e| Error::Crypto(e.to_string()))
    }

    async fn send<T: for<'de> Deserialize<'de>>(&self, action: &str, payload: Value) -> Result<T> {
        let payload = serde_json::to_string(&payload)?;
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        let date = utc_date(timestamp);
        let authorization = tc3_authorization(Tc3SignInput {
            secret_id: &self.credential.secret_id,
            secret_key: &self.credential.secret_key,
            method: "POST",
            canonical_uri: "/",
            canonical_query_string: "",
            canonical_header_prefix: "content-type:application/json; charset=utf-8\nhost:",
            host: &self.endpoint,
            payload: &payload,
            timestamp,
            date: &date,
        })?;
        let url = format!("https://{}", self.endpoint);
        let mut request = self
            .http
            .post(url)
            .header("Authorization", authorization)
            .header("Content-Type", "application/json; charset=utf-8")
            .header("Host", &self.endpoint)
            .header("X-TC-Action", action)
            .header("X-TC-Timestamp", timestamp.to_string())
            .header("X-TC-Version", "2019-01-18")
            .header("X-TC-Region", &self.region)
            .body(payload);
        if let Some(token) = &self.credential.session_token {
            request = request.header("X-TC-Token", token);
        }
        let response = request.send().await?;
        let status = response.status();
        let body = response.bytes().await?;
        if !status.is_success() {
            return Err(Error::Crypto(format!(
                "KMS HTTP {}: {}",
                status.as_u16(),
                String::from_utf8_lossy(&body)
            )));
        }
        Ok(serde_json::from_slice(&body)?)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KmsEncryptInner {
    ciphertext_blob: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KmsEncryptResponse {
    response: KmsEncryptInner,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KmsDecryptInner {
    plaintext: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct KmsDecryptResponse {
    response: KmsDecryptInner,
}

struct Tc3SignInput<'a> {
    secret_id: &'a str,
    secret_key: &'a str,
    method: &'a str,
    canonical_uri: &'a str,
    canonical_query_string: &'a str,
    canonical_header_prefix: &'a str,
    host: &'a str,
    payload: &'a str,
    timestamp: i64,
    date: &'a str,
}

fn tc3_authorization(input: Tc3SignInput<'_>) -> Result<String> {
    let canonical_headers = format!("{}{}\n", input.canonical_header_prefix, input.host);
    let signed_headers = "content-type;host";
    let hashed_payload = hex::encode(Sha256::digest(input.payload.as_bytes()));
    let canonical_request = format!(
        "{}\n{}\n{}\n{canonical_headers}\n{signed_headers}\n{hashed_payload}",
        input.method, input.canonical_uri, input.canonical_query_string
    );
    let credential_scope = format!("{}/kms/tc3_request", input.date);
    let string_to_sign = format!(
        "TC3-HMAC-SHA256\n{}\n{credential_scope}\n{}",
        input.timestamp,
        hex::encode(Sha256::digest(canonical_request.as_bytes()))
    );
    let secret_date = hmac_sha256(
        format!("TC3{}", input.secret_key).as_bytes(),
        input.date.as_bytes(),
    )?;
    let secret_service = hmac_sha256(&secret_date, b"kms")?;
    let secret_signing = hmac_sha256(&secret_service, b"tc3_request")?;
    let signature = hex::encode(hmac_sha256(&secret_signing, string_to_sign.as_bytes())?);
    Ok(format!(
        "TC3-HMAC-SHA256 Credential={}/{credential_scope}, SignedHeaders={signed_headers}, Signature={signature}",
        input.secret_id
    ))
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> Result<Vec<u8>> {
    use hmac::{Hmac, Mac};
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = <HmacSha256 as hmac::digest::KeyInit>::new_from_slice(key)
        .map_err(|e| Error::Crypto(format!("invalid HMAC key: {e}")))?;
    mac.update(data);
    Ok(mac.finalize().into_bytes().to_vec())
}

fn utc_date(timestamp: i64) -> String {
    // Civil date conversion from Unix days; avoids pulling chrono for one TC3 header.
    let days = timestamp.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let y = y + if m <= 2 { 1 } else { 0 };
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_date_formats_epoch_day() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(1_480_932_292), "2016-12-05");
    }
}
