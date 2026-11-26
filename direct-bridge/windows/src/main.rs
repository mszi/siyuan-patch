use anyhow::{Context, Result};
use clap::Parser;
use rustls::{pki_types::PrivateKeyDer, server::WebPkiClientVerifier, RootCertStore, ServerConfig};
use std::{fs::File, io::BufReader, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{io::copy_bidirectional, net::{TcpListener, TcpStream}};
use tokio_rustls::TlsAcceptor;

#[derive(Parser, Debug)]
#[command(name = "s3-direct-bridge-server", about = "mTLS TCP bridge to a local S3-compatible endpoint")]
struct Args {
    /// Public TCP address exposed by the router (must be forwarded to this port)
    #[arg(long, default_value = "0.0.0.0:24443")]
    listen: SocketAddr,
    /// Local BlindBucket/MinIO-compatible HTTP endpoint
    #[arg(long, default_value = "127.0.0.1:9008")]
    target: SocketAddr,
    /// PEM server certificate chain
    #[arg(long)]
    cert: PathBuf,
    /// PEM server private key
    #[arg(long)]
    key: PathBuf,
    /// PEM CA certificate used to authenticate Android client certificates
    #[arg(long)]
    client_ca: PathBuf,
}

fn load_certs(path: &PathBuf) -> Result<Vec<rustls::pki_types::CertificateDer<'static>>> {
    let mut reader = BufReader::new(File::open(path).with_context(|| format!("open {}", path.display()))?);
    Ok(rustls_pemfile::certs(&mut reader).collect::<std::result::Result<Vec<_>, _>>()?)
}

fn tls_acceptor(args: &Args) -> Result<TlsAcceptor> {
    let certs = load_certs(&args.cert)?;
    anyhow::ensure!(!certs.is_empty(), "server certificate chain is empty");
    let mut key_reader = BufReader::new(File::open(&args.key)?);
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_reader)?
        .context("no supported private key found in PEM file")?;
    let mut roots = RootCertStore::empty();
    for cert in load_certs(&args.client_ca)? {
        roots.add(cert).context("invalid client CA certificate")?;
    }
    let verifier = WebPkiClientVerifier::builder(Arc::new(roots)).build()
        .context("build client certificate verifier")?;
    let config = ServerConfig::builder()
        .with_client_cert_verifier(verifier)
        .with_single_cert(certs, key)
        .context("configure TLS server certificate")?;
    Ok(TlsAcceptor::from(Arc::new(config)))
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    let acceptor = tls_acceptor(&args)?;
    let listener = TcpListener::bind(args.listen).await
        .with_context(|| format!("bind {}", args.listen))?;
    eprintln!("Direct Bridge listening on {}; forwarding to {}", args.listen, args.target);
    loop {
        let (tcp, peer) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let target = args.target;
        tokio::spawn(async move {
            if let Err(err) = handle(tcp, peer, target, acceptor).await {
                eprintln!("[{peer}] connection closed: {err:#}");
            }
        });
    }
}

async fn handle(mut inbound: TcpStream, peer: SocketAddr, target: SocketAddr, acceptor: TlsAcceptor) -> Result<()> {
    let mut tls = acceptor.accept(inbound).await.context("TLS/mTLS handshake failed")?;
    let mut upstream = TcpStream::connect(target).await
        .with_context(|| format!("connect local S3 endpoint {target}"))?;
    let (up, down) = copy_bidirectional(&mut tls, &mut upstream).await
        .context("bidirectional TCP forwarding failed")?;
    eprintln!("[{peer}] tunnel closed; client->origin={up} bytes, origin->client={down} bytes");
    Ok(())
}
