#![cfg(test)]

use anyhow::{Context, Result, anyhow};
use memmem::{Searcher, TwoWaySearcher};
use rustls::{ClientConfig, KeyLog, pki_types::ServerName};
use rustls_platform_verifier::ConfigVerifierExt;
use std::{
    net::ToSocketAddrs,
    sync::{Arc, Mutex},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    time::{Duration, timeout},
};
use tokio_rustls::TlsConnector;
use url::Url;

use demo_web2_verifier::{
    Alpn, Artifact, CipherSuite, NssKeylog, NssKeylogValue, ProtocolVersion, TlsInfo,
};

use super::tls_tap::TlsTap;

// Insecure verifier (accept-any) used only when --insecure_no_verify is set.
#[derive(Debug)]
struct AcceptAllVerifier;

impl rustls::client::danger::ServerCertVerifier for AcceptAllVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        use rustls::SignatureScheme::*;
        vec![
            RSA_PSS_SHA256,
            ECDSA_NISTP256_SHA256,
            ED25519,
            RSA_PSS_SHA384,
            ECDSA_NISTP384_SHA384,
        ]
    }
}

pub struct Opts {
    /// Target URL (https://host[:port]/path?query)
    pub url: String,

    /// HTTP method
    pub method: String,

    /// Add request header (repeatable): -H "Name: value"
    pub headers: Vec<String>,

    /// Request body string (for POST/PUT/etc)
    pub body: Option<String>,

    /// Request body file path (mutually exclusive with --body)
    pub body_file: Option<String>,

    /// Verbose
    pub verbose: bool,

    /// Don't verify TLS certificates (INSECURE)
    pub insecure_no_verify: bool,

    /// Timeout in seconds for connect/handshake/read operations
    pub timeout: u64,
}

impl Default for Opts {
    fn default() -> Self {
        Self {
            url: "".into(),
            method: "GET".into(),
            headers: vec![],
            body: None,
            body_file: None,
            verbose: true,
            insecure_no_verify: false,
            timeout: 20,
        }
    }
}

#[derive(Debug, Default, Clone)]
struct InMemoryKeyLog {
    log: Arc<Mutex<NssKeylog>>,
}

impl InMemoryKeyLog {
    fn new() -> Self {
        Self {
            log: Arc::new(Mutex::new(NssKeylog::default())),
        }
    }

    fn take(self) -> NssKeylog {
        match Arc::try_unwrap(self.log) {
            Ok(m) => m.into_inner().unwrap(),
            Err(arc) => arc.lock().unwrap().clone(),
        }
    }
}

impl KeyLog for InMemoryKeyLog {
    fn will_log(&self, label: &str) -> bool {
        match label {
            "CLIENT_EARLY_TRAFFIC_SECRET" | "EARLY_EXPORTER_MASTER_SECRET" | "EXPORTER_SECRET" => {
                false
            }
            _ => true,
        }
    }

    fn log(&self, label: &str, client_random: &[u8], secret: &[u8]) {
        if let Ok(mut guard) = self.log.lock() {
            match label {
                _s if _s.starts_with("CLIENT_RANDOM") => {
                    guard.client_random = Some(NssKeylogValue {
                        random: client_random.to_vec(),
                        secret: secret.to_vec(),
                    });
                }
                _s if _s.starts_with("CLIENT_HANDSHAKE_TRAFFIC_SECRET") => {
                    guard.client_handshake_traffic_secret = Some(NssKeylogValue {
                        random: client_random.to_vec(),
                        secret: secret.to_vec(),
                    });
                }
                _s if _s.starts_with("SERVER_HANDSHAKE_TRAFFIC_SECRET") => {
                    guard.server_handshake_traffic_secret = Some(NssKeylogValue {
                        random: client_random.to_vec(),
                        secret: secret.to_vec(),
                    });
                }
                _s if _s.starts_with("CLIENT_TRAFFIC_SECRET") => {
                    guard.client_traffic_secrets.push(NssKeylogValue {
                        random: client_random.to_vec(),
                        secret: secret.to_vec(),
                    });
                }
                _s if _s.starts_with("SERVER_TRAFFIC_SECRET") => {
                    guard.server_traffic_secrets.push(NssKeylogValue {
                        random: client_random.to_vec(),
                        secret: secret.to_vec(),
                    });
                }
                _ => {}
            }
        }
    }
}

async fn read_http11_response_with_timeout<S: AsyncReadExt + Unpin>(
    mut stream: S,
    timeout_secs: u64,
) -> Result<(Option<String>, Vec<(String, String)>, Vec<u8>)> {
    use anyhow::anyhow;
    let idle = Duration::from_secs(timeout_secs);
    // 1) read headers until CRLFCRLF
    let mut buf = Vec::new();
    let mut tmp = [0u8; 8192];
    let mut headers_end = None;
    loop {
        let n = timeout(idle, stream.read(&mut tmp)).await??;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
        let searcher = TwoWaySearcher::new(b"\r\n\r\n");
        if let Some(pos) = searcher.search_in(&buf) {
            headers_end = Some(pos + 4);
            break;
        }
        if buf.len() > 1024 * 1024 {
            return Err(anyhow!("header too large"));
        }
    }
    let headers_end = headers_end.ok_or_else(|| anyhow!("incomplete headers"))?;
    let head = &buf[..headers_end];
    let mut body = buf[headers_end..].to_vec();

    // parse status line and headers
    let mut parts = head.split(|&b| b == b'\n');
    let status_line = parts
        .next()
        .map(|l| String::from_utf8_lossy(l).trim().to_string())
        .filter(|s| !s.is_empty());

    let mut headers = Vec::<(String, String)>::new();
    for line in parts {
        let l = String::from_utf8_lossy(line).trim().to_string();
        if l.is_empty() {
            continue;
        }
        if let Some(idx) = l.find(':') {
            let (n, v) = l.split_at(idx);
            headers.push((n.trim().to_string(), v[1..].trim().to_string()));
        }
    }

    // helpers
    let get_header = |name: &str| -> Option<String> {
        headers
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };

    // 2) decide body mode
    if let Some(te) = get_header("Transfer-Encoding") {
        if te.to_ascii_lowercase().contains("chunked") {
            // parse chunked
            let mut acc = Vec::new();
            let mut cursor = &body[..];
            loop {
                // read line with size
                let mut line = Vec::new();
                loop {
                    if let Some(p) = cursor.windows(2).position(|w| w == b"\r\n") {
                        line.extend_from_slice(&cursor[..p]);
                        cursor = &cursor[p + 2..];
                        break;
                    } else {
                        // need more
                        let n = timeout(idle, stream.read(&mut tmp)).await??;
                        if n == 0 {
                            break;
                        }
                        let mut newv = Vec::new();
                        newv.extend_from_slice(cursor);
                        newv.extend_from_slice(&tmp[..n]);
                        cursor = Box::leak(newv.into_boxed_slice());
                    }
                }
                let size_hex = String::from_utf8_lossy(&line);
                let size = usize::from_str_radix(size_hex.trim(), 16)
                    .map_err(|_| anyhow!("bad chunk size"))?;
                if size == 0 {
                    // consume trailing CRLF (already consumed by find)
                    break;
                }
                // ensure we have size bytes + CRLF
                while cursor.len() < size + 2 {
                    let n = timeout(idle, stream.read(&mut tmp)).await??;
                    if n == 0 {
                        return Err(anyhow!("eof mid-chunk"));
                    }
                    let mut newv = Vec::new();
                    newv.extend_from_slice(cursor);
                    newv.extend_from_slice(&tmp[..n]);
                    cursor = Box::leak(newv.into_boxed_slice());
                }
                acc.extend_from_slice(&cursor[..size]);
                cursor = &cursor[size + 2..];
            }
            body = acc;
        }
    } else if let Some(cl) = get_header("Content-Length") {
        let need: usize = cl.parse().map_err(|_| anyhow!("bad Content-Length"))?;
        while body.len() < need {
            let n = timeout(idle, stream.read(&mut tmp)).await??;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&tmp[..n]);
        }
        body.truncate(need);
    } else {
        // read-until-close with idle timeout
        loop {
            match timeout(idle, stream.read(&mut tmp)).await {
                Ok(Ok(0)) => break,
                Ok(Ok(n)) => body.extend_from_slice(&tmp[..n]),
                Ok(Err(e)) => return Err(e.into()),
                Err(_) => break, // idle timeout
            }
        }
    }

    Ok((status_line, headers, body))
}

pub async fn collect_request(opts: Opts) -> Result<Artifact> {
    if opts.body.is_some() && opts.body_file.is_some() {
        return Err(anyhow!("--body and --body_file are mutually exclusive"));
    }

    let url = Url::parse(&opts.url).context("invalid URL")?;
    if url.scheme() != "https" {
        return Err(anyhow!("only https:// scheme is supported"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("URL must have host"))?
        .to_string();
    let port = url.port().unwrap_or(443);
    let path_q = {
        let mut p = url.path().to_string();
        if let Some(q) = url.query() {
            p.push('?');
            p.push_str(q);
        }
        if p.is_empty() { "/".to_string() } else { p }
    };

    // Resolve DNS and connect TCP (with timeout)
    let mut addr_candidates = format!("{}:{}", host, port)
        .to_socket_addrs()
        .with_context(|| format!("DNS resolution failed for {}", host))?;
    let peer = addr_candidates
        .next()
        .ok_or_else(|| anyhow!("no addresses resolved"))?;

    let tcp = timeout(Duration::from_secs(opts.timeout), TcpStream::connect(peer))
        .await
        .context("TCP connect timed out")??;

    // Wrap in tap to capture encrypted bytes
    let tap = TlsTap::new(tcp);

    // Build ClientConfig with OS trust (rustls-platform-verifier) or insecure
    let mut config: ClientConfig = if opts.insecure_no_verify {
        let cfg = ClientConfig::builder()
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAllVerifier))
            .with_no_client_auth();
        cfg
    } else {
        ClientConfig::with_platform_verifier()
            .map_err(|e| anyhow!("failed to create ClientConfig: {}", e))?
    };

    // Enable NSS-style key logging
    let keylog = InMemoryKeyLog::new();
    config.key_log = Arc::new(keylog.clone());

    config.alpn_protocols = vec![b"http/1.1".to_vec()];

    let connector = TlsConnector::from(Arc::new(config));

    // SNI
    let sni: ServerName<'_> =
        ServerName::try_from(host.as_str()).map_err(|e| anyhow!("invalid SNI/host: {}", e))?;
    let sni_owned = sni.to_owned();

    // Do TLS handshake over our tap (with timeout)
    let mut tls_stream = timeout(
        Duration::from_secs(opts.timeout),
        connector.connect(sni_owned, tap),
    )
    .await
    .context("TLS handshake timed out")??;

    // Build HTTP/1.1 request
    let mut header = String::new();
    header.push_str(&format!("{} {} HTTP/1.1\r\n", opts.method, path_q));
    header.push_str(&format!("Host: {}\r\n", host));
    header.push_str("User-Agent: curl/8.15.0\r\n");
    header.push_str("Accept: */*\r\n");
    header.push_str("Connection: close\r\n");
    for h in &opts.headers {
        header.push_str(h);
        header.push_str("\r\n");
    }

    let body_bytes: Vec<u8> = if let Some(path) = &opts.body_file {
        std::fs::read(path).with_context(|| format!("read body file {}", path))?
    } else if let Some(b) = &opts.body {
        b.as_bytes().to_vec()
    } else {
        Vec::new()
    };

    if !body_bytes.is_empty() {
        header.push_str(&format!("Content-Length: {}\r\n", body_bytes.len()));
    }
    header.push_str("\r\n");

    let mut req_bytes = header.into_bytes();
    if !body_bytes.is_empty() {
        req_bytes.extend_from_slice(&body_bytes);
    }

    // Send request
    tls_stream.write_all(&req_bytes).await?;
    tls_stream.flush().await?;

    // Read HTTP/1.1 response with timeout
    let (status_line, headers_vec, body): (Option<String>, Vec<(String, String)>, Vec<u8>) =
        read_http11_response_with_timeout(&mut tls_stream, opts.timeout).await?;

    if opts.verbose {
        println!("Status: {:?}", status_line);
        println!("Headers:");
        for (header_name, header_value) in headers_vec {
            println!("{:?}: {:?}", header_name, header_value);
        }
        println!("Body:");
        println!("{:?}", String::from_utf8_lossy(&body));
    }

    // Extract TLS info
    let (_io_ref, conn) = tls_stream.get_ref();

    let alpn = conn
        .alpn_protocol()
        .map(Alpn::from)
        .ok_or_else(|| anyhow!("can't get alpn"))?;

    #[allow(non_snake_case)]
    let protocol_version = conn
        .protocol_version()
        .map(|v| match v {
            rustls::ProtocolVersion::TLSv1_2 => ProtocolVersion::TLSv1_2,
            rustls::ProtocolVersion::TLSv1_3 => ProtocolVersion::TLSv1_3,
            _ => ProtocolVersion::Unsupported,
        })
        .ok_or_else(|| anyhow!("can't get protocol version"))?;

    if opts.verbose {
        eprintln!(
            "Protocol version string: {:?}",
            conn.protocol_version().unwrap().as_str()
        );
    }

    #[allow(non_snake_case)]
    let cipher_suite = conn
        .negotiated_cipher_suite()
        .map(|s| match s.suite() {
            rustls::CipherSuite::TLS13_AES_128_GCM_SHA256 => CipherSuite::TLS_AES_128_GCM_SHA256,
            rustls::CipherSuite::TLS13_AES_256_GCM_SHA384 => CipherSuite::TLS_AES_256_GCM_SHA384,
            rustls::CipherSuite::TLS13_CHACHA20_POLY1305_SHA256 => {
                CipherSuite::TLS_CHACHA20_POLY1305_SHA256
            }
            _ => CipherSuite::Unsupported,
        })
        .ok_or_else(|| anyhow!("can't get cipher suite"))?;

    if opts.verbose {
        eprintln!(
            "Cipher suite string: {:?}",
            conn.negotiated_cipher_suite().unwrap().suite().as_str()
        );
    }

    // Certificates
    let mut peer_cert_chain = Vec::new();
    if let Some(chain) = conn.peer_certificates() {
        for cert in chain.iter() {
            peer_cert_chain.push(cert.to_vec());
        }
    }

    // Pull tap to get captured TLS records (encrypted)
    let tap = tls_stream.into_inner().0; // (IO, ClientConnection) -> IO is our TlsTap
    let (client_records, server_records) = tap.into_records();

    let artifact = Artifact {
        client_records,
        server_records,
        keylog: keylog.take(),
        info: TlsInfo {
            sni: host.into_bytes(),
            alpn,
            protocol_version,
            cipher_suite,
            peer_cert_chain,
        },
    };

    Ok(artifact)
}
