use super::*;

/// A self-signed PEM certificate, used only to check that a custom trust
/// anchor is parsed and accepted. It is never presented to a peer.
const SAMPLE_PEM: &str = include_str!("testdata/sample-ca.pem");

/// Serializes the tests that observe or steer the platform certificate
/// store, which `SSL_CERT_DIR`/`SSL_CERT_FILE` make process-global state.
fn platform_store_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[test]
fn platform_trust_store_alone_builds_a_tls_config() {
    let _guard = platform_store_lock();
    let downloader = RustlsDownloader::new("diotypst-test");

    assert!(downloader.tls_config().is_ok());
}

#[test]
fn a_custom_certificate_joins_the_trust_roots() {
    let _guard = platform_store_lock();
    let file = tempfile::NamedTempFile::new().expect("temp file should be created");
    std::fs::write(file.path(), SAMPLE_PEM).expect("certificate should be written");

    let downloader = RustlsDownloader::with_cert_path("diotypst-test", file.path());

    let plain_roots = RustlsDownloader::new("diotypst-test")
        .trust_roots()
        .expect("platform roots should load")
        .len();
    let with_cert = downloader
        .trust_roots()
        .expect("custom certificate should be accepted")
        .len();

    assert_eq!(with_cert, plain_roots + 1);
    assert!(downloader.tls_config().is_ok());
}

#[test]
fn an_unreadable_certificate_path_is_reported_not_ignored() {
    let downloader = RustlsDownloader::with_cert_path("diotypst-test", "/nonexistent/ca.pem");

    assert!(matches!(
        downloader.trust_roots(),
        Err(RustlsCertError::Read { .. })
    ));
}

#[test]
fn a_file_without_a_certificate_is_reported_not_ignored() {
    let file = tempfile::NamedTempFile::new().expect("temp file should be created");
    std::fs::write(file.path(), b"not a certificate").expect("content should be written");

    let downloader = RustlsDownloader::with_cert_path("diotypst-test", file.path());

    assert!(matches!(
        downloader.trust_roots(),
        Err(RustlsCertError::Parse { .. })
    ));
}

#[test]
fn a_corrupt_certificate_is_rejected_not_silently_dropped() {
    // PEM framing only base64-decodes: this passes `pem_file_iter` and is then
    // refused by rustls as DER. Discarding that refusal would leave a caller
    // believing a trust anchor is installed when it is not.
    let file = tempfile::NamedTempFile::new().expect("temp file should be created");
    std::fs::write(
        file.path(),
        "-----BEGIN CERTIFICATE-----\nbm90IGEgY2VydGlmaWNhdGU=\n-----END CERTIFICATE-----\n",
    )
    .expect("content should be written");

    let downloader = RustlsDownloader::with_cert_path("diotypst-test", file.path());

    assert!(matches!(
        downloader.trust_roots(),
        Err(RustlsCertError::Rejected { .. })
    ));
    assert!(downloader.tls_config().is_err());
}

#[test]
fn a_trust_store_with_no_anchors_is_an_error() {
    // SSL_CERT_FILE/SSL_CERT_DIR steer rustls-native-certs, so an empty
    // directory produces a platform store that yields nothing. Building a TLS
    // config from zero anchors would fail every connection at handshake time
    // with no hint as to why.
    let _guard = platform_store_lock();
    let empty = tempfile::tempdir().expect("temp dir should be created");
    let restore = EnvGuard::set("SSL_CERT_DIR", empty.path().as_os_str());
    let restore_file = EnvGuard::set("SSL_CERT_FILE", "".as_ref());

    let downloader = RustlsDownloader::new("diotypst-test");
    let roots = downloader.trust_roots();

    drop(restore_file);
    drop(restore);

    assert!(matches!(roots, Err(RustlsCertError::NoTrustAnchors { .. })));
}

/// Restores an environment variable when dropped, so one test's steering of
/// the certificate lookup cannot leak into another.
struct EnvGuard {
    key: &'static str,
    previous: Option<std::ffi::OsString>,
}

impl EnvGuard {
    fn set(key: &'static str, value: &std::ffi::OsStr) -> Self {
        let previous = std::env::var_os(key);
        unsafe { std::env::set_var(key, value) };
        Self { key, previous }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        match &self.previous {
            Some(value) => unsafe { std::env::set_var(self.key, value) },
            None => unsafe { std::env::remove_var(self.key) },
        }
    }
}

/// Serves one canned HTTP response on a loopback port and returns its URL.
///
/// `stream` is exercised over plain HTTP: the status and header mapping under
/// test is transport-independent, and a TLS listener would test rustls rather
/// than this crate.
fn serve_once(response: &'static str) -> (String, std::thread::JoinHandle<()>) {
    use std::io::Write;

    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("loopback listener should bind");
    let port = listener
        .local_addr()
        .expect("listener has an address")
        .port();
    let handle = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut discard = [0_u8; 1024];
            let _ = std::io::Read::read(&mut stream, &mut discard);
            let _ = stream.write_all(response.as_bytes());
        }
    });

    (format!("http://127.0.0.1:{port}/package.tar.gz"), handle)
}

#[test]
fn a_download_reports_its_body_and_content_length() {
    let (url, server) =
        serve_once("HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\nhello world");
    let cert = sample_cert();
    let downloader = RustlsDownloader::with_cert_path("diotypst-test", cert.path());

    let (length, mut body) = downloader
        .stream(&(), &url)
        .expect("a 200 response should stream");
    let mut received = Vec::new();
    std::io::Read::read_to_end(&mut body, &mut received).expect("body should read");

    assert_eq!(length, Some(11));
    assert_eq!(received, b"hello world");
    server.join().expect("server thread should finish");
}

#[test]
fn a_missing_package_maps_to_the_not_found_kind() {
    // `RegistryPackages` keys `PackageResolveError::NotFound` off this exact
    // io::ErrorKind, so the registry's 404 has to survive as NotFound and not
    // collapse into a generic transport error.
    let (url, server) =
        serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
    let cert = sample_cert();
    let downloader = RustlsDownloader::with_cert_path("diotypst-test", cert.path());

    let error = match downloader.stream(&(), &url) {
        Err(error) => error,
        Ok(_) => panic!("a 404 response should fail"),
    };

    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    server.join().expect("server thread should finish");
}

/// Writes the sample certificate to a temp file, so trust-root assembly does
/// not depend on the host having a populated platform store.
fn sample_cert() -> tempfile::NamedTempFile {
    let file = tempfile::NamedTempFile::new().expect("temp file should be created");
    std::fs::write(file.path(), SAMPLE_PEM).expect("certificate should be written");
    file
}

#[test]
fn a_stalled_response_body_fails_instead_of_hanging() {
    // A registry that sends headers and then goes silent used to hold the
    // download open forever: ureq applies no read timeout by default.
    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("loopback listener should bind");
    let port = listener
        .local_addr()
        .expect("listener has an address")
        .port();
    let (stop, stopped) = std::sync::mpsc::channel::<()>();
    let server = std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut discard = [0_u8; 1024];
            let _ = std::io::Read::read(&mut stream, &mut discard);
            // Promise a body, then never send it.
            let _ = std::io::Write::write_all(
                &mut stream,
                b"HTTP/1.1 200 OK\r\nContent-Length: 64\r\n\r\n",
            );
            let _ = stopped.recv_timeout(std::time::Duration::from_secs(10));
        }
    });

    let cert = sample_cert();
    let downloader = RustlsDownloader::with_cert_path("diotypst-test", cert.path())
        .read_timeout(std::time::Duration::from_millis(250));

    let outcome = downloader.stream(&(), &format!("http://127.0.0.1:{port}/package.tar.gz"));
    // The stall may surface when the headers are read or while draining the
    // promised body; either way it must terminate rather than block.
    let error = match outcome {
        Err(error) => error,
        Ok((_, mut body)) => {
            let mut sink = Vec::new();
            std::io::Read::read_to_end(&mut body, &mut sink)
                .expect_err("a stalled body should not read to completion")
        }
    };
    assert!(
        matches!(
            error.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        ),
        "expected a timeout, got {:?}: {error}",
        error.kind()
    );

    let _ = stop.send(());
    server.join().expect("server thread should finish");
}
