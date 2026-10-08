/*
 * Copyright 2026 Federico D'Ambrosio
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Optional TLS settings for the Prometheus connection: a CA bundle trusted in
//! place of the built-in roots, and a client certificate for mutual TLS.

use anyhow::{Result, anyhow, bail};
use reqwest::{Certificate, ClientBuilder, Identity};
use rustls::{AlertDescription, CertificateError};
use rustls_pki_types::pem::{self, SectionKind};
use serde::Deserialize;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// TLS files for the Prometheus connection, and the config's `[tls]` table.
/// Each is optional; with none set, the connection trusts the built-in roots
/// and sends no client certificate. Unknown keys are rejected, so a misspelled
/// option cannot leave the connection without its certificates.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TlsFiles {
    /// CA certificates trusted instead of the built-in roots.
    pub(crate) ca_cert: Option<PathBuf>,
    /// The client certificate, followed by any intermediate CAs.
    pub(crate) client_cert: Option<PathBuf>,
    /// The client certificate's private key.
    pub(crate) client_key: Option<PathBuf>,
}

impl TlsFiles {
    pub(crate) fn is_empty(&self) -> bool {
        self.ca_cert.is_none() && self.client_cert.is_none() && self.client_key.is_none()
    }

    /// These files, taking each one that is unset from `fallback`.
    pub(crate) fn or(self, fallback: Self) -> Self {
        Self {
            ca_cert: self.ca_cert.or(fallback.ca_cert),
            client_cert: self.client_cert.or(fallback.client_cert),
            client_key: self.client_key.or(fallback.client_key),
        }
    }

    pub(crate) fn map_paths(self, f: impl Fn(&Path) -> PathBuf) -> Self {
        Self {
            ca_cert: self.ca_cert.as_deref().map(&f),
            client_cert: self.client_cert.as_deref().map(&f),
            client_key: self.client_key.as_deref().map(&f),
        }
    }

    /// The options that are set, for error messages.
    fn option_names(&self) -> String {
        [
            ("ca_cert", &self.ca_cert),
            ("client_cert", &self.client_cert),
            ("client_key", &self.client_key),
        ]
        .into_iter()
        .filter(|(_, path)| path.is_some())
        .map(|(name, _)| name)
        .collect::<Vec<_>>()
        .join(", ")
    }

    /// Describes `error`, from building a client configured with these files.
    pub(super) fn build_error(&self, error: &reqwest::Error) -> anyhow::Error {
        if let (Some(cert), Some(key)) = (&self.client_cert, &self.client_key)
            && matches!(
                find_rustls_error(error),
                Some(rustls::Error::InconsistentKeys(_))
            )
        {
            return anyhow!(
                "client_key {} is not the private key of client_cert {}",
                key.display(),
                cert.display()
            );
        }
        anyhow!(
            "configuring TLS ({}): {}",
            self.option_names(),
            error_chain(error)
        )
    }

    /// Configures `builder` to connect to `url` with these files. Each file is
    /// read and checked first, so an error names the file and what is wrong.
    pub(super) fn configure(&self, url: &str, mut builder: ClientBuilder) -> Result<ClientBuilder> {
        if self.is_empty() {
            return Ok(builder);
        }
        if !url.trim().to_ascii_lowercase().starts_with("https://") {
            // Only the scheme is shown, since the URL may hold credentials.
            let scheme = url
                .trim()
                .split_once("://")
                .map_or("no scheme".to_string(), |(scheme, _)| {
                    format!("{scheme}://")
                });
            bail!(
                "TLS options ({}) need an https:// Prometheus URL, but prometheus_url uses {scheme}",
                self.option_names()
            );
        }

        if let Some(path) = &self.ca_cert {
            let file = PemFile::read("ca_cert", path)?;
            file.require_certificate()?;
            let certs = Certificate::from_pem_bundle(&file.bytes)
                .map_err(|error| file.error(&error_chain(&error)))?;
            builder = builder.tls_built_in_root_certs(false);
            for cert in certs {
                builder = builder.add_root_certificate(cert);
            }
        }

        match (&self.client_cert, &self.client_key) {
            (Some(cert_path), Some(key_path)) => {
                let cert = PemFile::read("client_cert", cert_path)?;
                cert.require_certificate()?;
                let key = PemFile::read("client_key", key_path)?;
                key.require_private_key()?;
                let mut pem = cert.bytes;
                pem.push(b'\n');
                pem.extend_from_slice(&key.bytes);
                let identity = Identity::from_pem(&pem).map_err(|error| {
                    anyhow!(
                        "client_cert {} and client_key {}: {}",
                        cert_path.display(),
                        key_path.display(),
                        error_chain(&error)
                    )
                })?;
                builder = builder.identity(identity);
            }
            (Some(_), None) => {
                bail!("client_cert is set without client_key; a client certificate needs both")
            }
            (None, Some(_)) => {
                bail!("client_key is set without client_cert; a client certificate needs both")
            }
            (None, None) => {}
        }
        Ok(builder)
    }
}

/// A PEM file named by a TLS option, with the kinds of sections it holds.
struct PemFile<'a> {
    option: &'static str,
    path: &'a Path,
    bytes: Vec<u8>,
    sections: Vec<SectionKind>,
}

impl<'a> PemFile<'a> {
    fn read(option: &'static str, path: &'a Path) -> Result<Self> {
        let bytes =
            std::fs::read(path).map_err(|error| anyhow!("{option} {}: {error}", path.display()))?;
        let mut file = Self {
            option,
            path,
            bytes,
            sections: Vec::new(),
        };
        let mut cursor = Cursor::new(&file.bytes);
        loop {
            match pem::from_buf(&mut cursor) {
                Ok(Some((kind, _))) => file.sections.push(kind),
                Ok(None) => break,
                // The parser's errors can quote the file, which may be a key.
                Err(_) => return Err(file.error("not a valid PEM file")),
            }
        }
        Ok(file)
    }

    fn error(&self, reason: &str) -> anyhow::Error {
        anyhow!("{} {}: {reason}", self.option, self.path.display())
    }

    fn require_certificate(&self) -> Result<()> {
        if self.sections.contains(&SectionKind::Certificate) {
            Ok(())
        } else {
            Err(self.error("no PEM certificate found (expected -----BEGIN CERTIFICATE-----)"))
        }
    }

    fn require_private_key(&self) -> Result<()> {
        let keys = self
            .sections
            .iter()
            .filter(|kind| {
                matches!(
                    kind,
                    SectionKind::PrivateKey
                        | SectionKind::EcPrivateKey
                        | SectionKind::RsaPrivateKey
                )
            })
            .count();
        match keys {
            1 => Ok(()),
            0 if contains(&self.bytes, b"-----BEGIN ENCRYPTED PRIVATE KEY-----") => Err(self
                .error(
                    "the private key is encrypted, which is not supported; \
                 decrypt it with `openssl pkey -in <file> -out <file>`",
                )),
            0 => Err(self.error(
                "no PEM private key found (expected -----BEGIN PRIVATE KEY-----, \
                 or an EC or RSA PRIVATE KEY)",
            )),
            _ => Err(self.error(&format!("{keys} private keys found; expected one"))),
        }
    }
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// A failed TLS handshake, or a server rejecting the client certificate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TlsFailure {
    /// A few words for the title bar, such as `TLS: unknown issuer`.
    pub(super) cause: String,
    /// What to check.
    pub(super) hint: &'static str,
}

impl TlsFailure {
    /// The TLS failure among `error`'s sources, if any.
    pub(super) fn find(error: &(dyn std::error::Error + 'static)) -> Option<Self> {
        find_rustls_error(error).map(Self::new)
    }

    fn new(error: &rustls::Error) -> Self {
        let failure = |cause: &str, hint| Self {
            cause: format!("TLS: {cause}"),
            hint,
        };
        match error {
            rustls::Error::InvalidCertificate(CertificateError::UnknownIssuer) => failure(
                "unknown issuer",
                "the server's certificate is not signed by a CA in ca_cert, \
                 or in the built-in roots when ca_cert is unset",
            ),
            rustls::Error::InvalidCertificate(
                CertificateError::NotValidForName | CertificateError::NotValidForNameContext { .. },
            ) => failure(
                "certificate not valid for this host",
                "the server's certificate has no subject alternative name \
                 for the host or IP address in prometheus_url",
            ),
            rustls::Error::InvalidCertificate(
                CertificateError::Expired | CertificateError::ExpiredContext { .. },
            ) => failure(
                "server certificate expired",
                "renew the server's certificate",
            ),
            rustls::Error::InvalidCertificate(error) => failure(
                &format!("invalid server certificate ({error:?})"),
                "check the server's certificate",
            ),
            rustls::Error::AlertReceived(AlertDescription::CertificateRequired) => failure(
                "client certificate required",
                "the server requires a client certificate; set client_cert and client_key",
            ),
            rustls::Error::AlertReceived(
                alert @ (AlertDescription::BadCertificate
                | AlertDescription::UnknownCA
                | AlertDescription::UnsupportedCertificate
                | AlertDescription::CertificateRevoked
                | AlertDescription::CertificateExpired
                | AlertDescription::CertificateUnknown
                | AlertDescription::AccessDenied),
            ) => failure(
                &format!("client certificate rejected ({alert:?})"),
                "the server does not accept client_cert; check that a CA the server \
                 trusts issued it and that it has not expired",
            ),
            rustls::Error::AlertReceived(alert) => failure(
                &format!("server sent alert {alert:?}"),
                "check the server's TLS settings",
            ),
            error => failure(&error.to_string(), "check the server's TLS settings"),
        }
    }
}

/// The rustls error among `error`'s sources, if any. rustls errors can arrive
/// wrapped in one or more `io::Error`s, whose `source` skips them.
fn find_rustls_error<'a>(
    error: &'a (dyn std::error::Error + 'static),
) -> Option<&'a rustls::Error> {
    let mut next = Some(error);
    while let Some(error) = next {
        if let Some(error) = error.downcast_ref::<rustls::Error>() {
            return Some(error);
        }
        if let Some(found) = error
            .downcast_ref::<std::io::Error>()
            .and_then(std::io::Error::get_ref)
            .and_then(|inner| find_rustls_error(inner))
        {
            return Some(found);
        }
        next = error.source();
    }
    None
}

/// `error` and its sources, joined with `: `.
pub(super) fn error_chain(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        text.push_str(": ");
        text.push_str(&error.to_string());
        source = error.source();
    }
    text
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::prom::{PromClient, QueryResult, is_transport_error, tls_failure_cause};
    use rcgen::{
        BasicConstraints, CertificateParams, CertifiedIssuer, ExtendedKeyUsagePurpose, IsCa,
        KeyPair,
    };
    use rustls_pki_types::pem::PemObject;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
    use tokio_rustls::rustls;

    /// A test certificate authority.
    pub(crate) struct TestCa {
        issuer: CertifiedIssuer<'static, KeyPair>,
    }

    /// A certificate chain and its PKCS#8 key, in PEM.
    pub(crate) struct TestCert {
        pub(crate) chain_pem: String,
        pub(crate) key_pem: String,
    }

    impl TestCa {
        pub(crate) fn new(name: &str) -> Self {
            let mut params = CertificateParams::new(Vec::<String>::new()).unwrap();
            params
                .distinguished_name
                .push(rcgen::DnType::CommonName, name);
            params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
            let issuer =
                CertifiedIssuer::self_signed(params, KeyPair::generate().unwrap()).unwrap();
            Self { issuer }
        }

        pub(crate) fn pem(&self) -> String {
            self.issuer.pem()
        }

        /// A certificate for `127.0.0.1` (an IP SAN), followed by this CA.
        pub(crate) fn issue(&self, usage: ExtendedKeyUsagePurpose) -> TestCert {
            let mut params = CertificateParams::new(vec!["127.0.0.1".to_string()]).unwrap();
            params.extended_key_usages = vec![usage];
            let key = KeyPair::generate().unwrap();
            let cert = params.signed_by(&key, &self.issuer).unwrap();
            TestCert {
                chain_pem: format!("{}{}", cert.pem(), self.pem()),
                key_pem: key.serialize_pem(),
            }
        }

        pub(crate) fn server_cert(&self) -> TestCert {
            self.issue(ExtendedKeyUsagePurpose::ServerAuth)
        }

        pub(crate) fn client_cert(&self) -> TestCert {
            self.issue(ExtendedKeyUsagePurpose::ClientAuth)
        }
    }

    const UP: &str = r#"{"status":"success","data":{"resultType":"vector","result":[{"metric":{"job":"node"},"value":[1,"1"]}]}}"#;

    /// Reads one request and answers it with [`UP`].
    async fn answer(mut stream: impl AsyncRead + AsyncWrite + Unpin) {
        let mut request = Vec::new();
        let mut buf = [0; 1024];
        while !contains(&request, b"\r\n\r\n") {
            match stream.read(&mut buf).await {
                Ok(0) | Err(_) => return,
                Ok(n) => request.extend_from_slice(&buf[..n]),
            }
        }
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
             content-length: {}\r\nconnection: close\r\n\r\n{UP}",
            UP.len()
        );
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    }

    /// A Prometheus stand-in like a hardened node: TLS 1.3 only on
    /// `https://127.0.0.1`, requiring a client certificate that `client_ca`
    /// issued. Also returns the count of accepted connections.
    pub(crate) async fn mtls_prometheus(
        server: &TestCert,
        client_ca: &TestCa,
    ) -> (String, Arc<AtomicUsize>) {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let mut roots = rustls::RootCertStore::empty();
        roots.add(client_ca.issuer.der().clone()).unwrap();
        let verifier = rustls::server::WebPkiClientVerifier::builder_with_provider(
            Arc::new(roots),
            Arc::clone(&provider),
        )
        .build()
        .unwrap();
        let chain = rustls_pki_types::CertificateDer::pem_slice_iter(server.chain_pem.as_bytes())
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        let key =
            rustls_pki_types::PrivateKeyDer::from_pem_slice(server.key_pem.as_bytes()).unwrap();
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_protocol_versions(&[&rustls::version::TLS13])
            .unwrap()
            .with_client_cert_verifier(verifier)
            .with_single_cert(chain, key)
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let connections = Arc::new(AtomicUsize::new(0));
        let accepted = Arc::clone(&connections);
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                accepted.fetch_add(1, Ordering::SeqCst);
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    if let Ok(stream) = acceptor.accept(stream).await {
                        answer(stream).await;
                    }
                });
            }
        });
        (format!("https://{address}"), connections)
    }

    async fn plain_prometheus() -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                tokio::spawn(answer(stream));
            }
        });
        format!("http://{address}")
    }

    /// A fresh directory for one test's files.
    pub(crate) fn test_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("grafana-tui-tls-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    pub(crate) fn write(dir: &Path, name: &str, contents: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    /// The files for trusting `ca` and, when given, presenting `client`.
    pub(crate) fn tls_files(dir: &Path, ca: &str, client: Option<&TestCert>) -> TlsFiles {
        TlsFiles {
            ca_cert: Some(write(dir, "ca.pem", ca)),
            client_cert: client.map(|client| write(dir, "client.pem", &client.chain_pem)),
            client_key: client.map(|client| write(dir, "client.key", &client.key_pem)),
        }
    }

    async fn query_up(url: &str, files: &TlsFiles) -> anyhow::Result<QueryResult> {
        PromClient::with_tls(url.to_string(), files)?
            .query_instant_series("up", 1)
            .await
    }

    fn config_error(url: &str, files: &TlsFiles) -> String {
        PromClient::with_tls(url.to_string(), files)
            .unwrap_err()
            .to_string()
    }

    #[tokio::test]
    async fn a_trusted_client_certificate_is_accepted() {
        let (server_ca, client_ca) = (TestCa::new("server CA"), TestCa::new("client CA"));
        let (url, _) = mtls_prometheus(&server_ca.server_cert(), &client_ca).await;
        let dir = test_dir("accepted");

        let files = tls_files(&dir, &server_ca.pem(), Some(&client_ca.client_cert()));
        let result = query_up(&url, &files).await.unwrap();

        assert_eq!(result.series.len(), 1);
        assert_eq!(result.series[0].metric["job"], "node");
    }

    #[tokio::test]
    async fn without_a_ca_the_server_has_an_unknown_issuer() {
        let (server_ca, client_ca) = (TestCa::new("server CA"), TestCa::new("client CA"));
        let (url, _) = mtls_prometheus(&server_ca.server_cert(), &client_ca).await;

        let error = query_up(&url, &TlsFiles::default()).await.unwrap_err();

        assert!(is_transport_error(&error));
        assert_eq!(tls_failure_cause(&error), Some("TLS: unknown issuer"));
        assert!(error.to_string().contains("ca_cert"), "{error}");
    }

    #[tokio::test]
    async fn without_a_client_certificate_the_server_rejects_the_connection() {
        let (server_ca, client_ca) = (TestCa::new("server CA"), TestCa::new("client CA"));
        let (url, connections) = mtls_prometheus(&server_ca.server_cert(), &client_ca).await;
        let dir = test_dir("no-client-cert");
        let client = PromClient::with_tls(url, &tls_files(&dir, &server_ca.pem(), None)).unwrap();

        let error = client
            .query_range("up", 0, 60, std::time::Duration::from_secs(15))
            .await
            .unwrap_err();

        assert!(is_transport_error(&error));
        assert_eq!(
            tls_failure_cause(&error),
            Some("TLS: client certificate required")
        );
        assert!(error.to_string().contains("client_cert and client_key"));
        // The certificates won't change between attempts, so it isn't retried.
        assert_eq!(connections.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_client_certificate_from_another_ca_is_rejected() {
        let (server_ca, client_ca) = (TestCa::new("server CA"), TestCa::new("client CA"));
        let (url, _) = mtls_prometheus(&server_ca.server_cert(), &client_ca).await;
        let dir = test_dir("other-ca");
        let stranger = TestCa::new("other CA").client_cert();

        let files = tls_files(&dir, &server_ca.pem(), Some(&stranger));
        let error = query_up(&url, &files).await.unwrap_err();

        assert_eq!(
            tls_failure_cause(&error),
            Some("TLS: client certificate rejected (UnknownCA)")
        );
    }

    #[tokio::test]
    async fn a_two_ca_bundle_trusts_a_server_signed_by_either_ca() {
        let (old_ca, new_ca) = (TestCa::new("old CA"), TestCa::new("new CA"));
        let client_ca = TestCa::new("client CA");
        let client = client_ca.client_cert();
        let dir = test_dir("rotation");
        let bundle = format!("{}{}", old_ca.pem(), new_ca.pem());
        let files = tls_files(&dir, &bundle, Some(&client));

        for server_ca in [&old_ca, &new_ca] {
            let (url, _) = mtls_prometheus(&server_ca.server_cert(), &client_ca).await;
            assert_eq!(query_up(&url, &files).await.unwrap().series.len(), 1);
        }
    }

    #[tokio::test]
    async fn without_tls_options_plain_http_is_unchanged() {
        let url = plain_prometheus().await;

        let result = query_up(&url, &TlsFiles::default()).await.unwrap();

        assert_eq!(result.series.len(), 1);
    }

    #[test]
    fn tls_options_need_an_https_url() {
        let dir = test_dir("http-url");
        let files = tls_files(&dir, &TestCa::new("CA").pem(), None);

        let error = config_error("http://admin:secret@10.60.1.2:9090", &files);

        assert_eq!(
            error,
            "TLS options (ca_cert) need an https:// Prometheus URL, but prometheus_url uses http://"
        );
        assert!(config_error("10.60.1.2:9090", &files).ends_with("uses no scheme"));
        assert!(PromClient::with_tls("HTTPS://10.60.1.2:9090".into(), &files).is_ok());
    }

    #[test]
    fn a_client_certificate_needs_its_key() {
        let dir = test_dir("half-identity");
        let client = TestCa::new("CA").client_cert();
        let cert = write(&dir, "client.pem", &client.chain_pem);
        let key = write(&dir, "client.key", &client.key_pem);
        let url = "https://10.60.1.2:9090";

        let cert_only = TlsFiles {
            client_cert: Some(cert),
            ..TlsFiles::default()
        };
        assert_eq!(
            config_error(url, &cert_only),
            "client_cert is set without client_key; a client certificate needs both"
        );
        let key_only = TlsFiles {
            client_key: Some(key),
            ..TlsFiles::default()
        };
        assert_eq!(
            config_error(url, &key_only),
            "client_key is set without client_cert; a client certificate needs both"
        );
    }

    #[test]
    fn file_errors_name_the_option_and_the_file() {
        let dir = test_dir("file-errors");
        let client = TestCa::new("CA").client_cert();
        let url = "https://10.60.1.2:9090";
        let missing = dir.join("missing.pem");
        let key_path = write(&dir, "key-only.pem", &client.key_pem);
        let cert_path = write(&dir, "cert-only.pem", &client.chain_pem);
        let encrypted = write(
            &dir,
            "encrypted.key",
            "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----\n",
        );
        let two_keys = write(
            &dir,
            "two.key",
            &format!("{}{}", client.key_pem, client.key_pem),
        );
        let garbled = write(&dir, "garbled.pem", "-----BEGIN CERTIFICATE-----\n!!!\n");
        let client_files = |key: &Path| TlsFiles {
            client_cert: Some(cert_path.clone()),
            client_key: Some(key.to_path_buf()),
            ..TlsFiles::default()
        };
        let ca_file = |ca: &Path| TlsFiles {
            ca_cert: Some(ca.to_path_buf()),
            ..TlsFiles::default()
        };

        let error = config_error(url, &ca_file(&missing));
        assert!(
            error.starts_with(&format!("ca_cert {}: ", missing.display())),
            "{error}"
        );
        assert_eq!(
            config_error(url, &ca_file(&key_path)),
            format!(
                "ca_cert {}: no PEM certificate found (expected -----BEGIN CERTIFICATE-----)",
                key_path.display()
            )
        );
        assert_eq!(
            config_error(url, &ca_file(&garbled)),
            format!("ca_cert {}: not a valid PEM file", garbled.display())
        );
        let no_cert = TlsFiles {
            client_cert: Some(key_path.clone()),
            client_key: Some(key_path.clone()),
            ..TlsFiles::default()
        };
        assert!(config_error(url, &no_cert).starts_with(&format!(
            "client_cert {}: no PEM certificate found",
            key_path.display()
        )));

        let error = config_error(url, &client_files(&cert_path));
        assert!(
            error.starts_with(&format!(
                "client_key {}: no PEM private key found",
                cert_path.display()
            )),
            "{error}"
        );
        assert!(config_error(url, &client_files(&encrypted)).contains("is encrypted"));
        assert!(config_error(url, &client_files(&two_keys)).contains("2 private keys found"));
    }

    #[test]
    fn errors_never_include_key_material() {
        let dir = test_dir("no-key-material");
        let client = TestCa::new("CA").client_cert();
        let body: String = client
            .key_pem
            .lines()
            .filter(|line| !line.starts_with("-----"))
            .collect();
        // A key with a truncated body, given as both the certificate and the key.
        let broken = format!("{}\n-----BEGIN CERTIFICATE-----\n", client.key_pem);
        let path = write(&dir, "broken.pem", &broken);
        let files = TlsFiles {
            client_cert: Some(path.clone()),
            client_key: Some(path),
            ..TlsFiles::default()
        };

        let error = config_error("https://10.60.1.2:9090", &files);

        for line in body.as_bytes().chunks(16) {
            assert!(
                !error.contains(std::str::from_utf8(line).unwrap()),
                "{error}"
            );
        }
    }

    #[test]
    fn a_key_for_another_certificate_is_a_config_error() {
        let dir = test_dir("key-mismatch");
        let client_ca = TestCa::new("client CA");
        let (cert, other) = (client_ca.client_cert(), client_ca.client_cert());
        let files = TlsFiles {
            ca_cert: None,
            client_cert: Some(write(&dir, "client.pem", &cert.chain_pem)),
            client_key: Some(write(&dir, "client.key", &other.key_pem)),
        };

        assert_eq!(
            config_error("https://10.60.1.2:9090", &files),
            format!(
                "client_key {} is not the private key of client_cert {}",
                dir.join("client.key").display(),
                dir.join("client.pem").display()
            )
        );
    }

    #[test]
    fn cli_options_override_config_options_one_by_one() {
        let cli = TlsFiles {
            client_cert: Some(PathBuf::from("~/cli.pem")),
            ..TlsFiles::default()
        };
        let config = TlsFiles {
            ca_cert: Some(PathBuf::from("/etc/ca.pem")),
            client_cert: Some(PathBuf::from("/etc/client.pem")),
            client_key: Some(PathBuf::from("~/client.key")),
        };

        let files = cli.or(config).map_paths(crate::config::expand_path);

        assert_eq!(files.ca_cert, Some(PathBuf::from("/etc/ca.pem")));
        assert_eq!(
            files.client_cert,
            Some(crate::config::expand_path(Path::new("~/cli.pem")))
        );
        assert_ne!(files.client_key, Some(PathBuf::from("~/client.key")));
        assert!(files.client_key.unwrap().ends_with("client.key"));
    }
}
