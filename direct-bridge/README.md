# S3 Direct Bridge (development branch)

This directory contains the Windows-side TCP/TLS forwarder. It does not relay traffic through a cloud service: the Android client connects directly to the forwarded TCP port on the home router, and this process forwards the decrypted stream to the local S3-compatible endpoint.

## Windows build

Install Rust stable (MSVC toolchain), then from this directory:

```powershell
cargo build --release
```

The executable is `target\release\s3-direct-bridge-server.exe`.

## Run

Create a server certificate and a separate client CA/certificate. Keep the server private key and client CA private material on the Windows host. The Android app must use a client certificate issued by the configured client CA and pin the server certificate SHA-256 fingerprint.

```powershell
.\s3-direct-bridge-server.exe --listen 0.0.0.0:24443 --target 127.0.0.1:9008 --cert server-chain.pem --key server-key.pem --client-ca android-client-ca.pem
```

Forward **TCP** port 24443 from the router to this Windows host. UDP/STUN mapping alone does not make this TCP listener reachable. Keep BlindBucket bound to loopback when possible.

## Security model

* TLS 1.3/1.2 is provided by rustls; mutual TLS client authentication is mandatory.
* No third-party relay is involved.
* The proxy is byte-transparent: S3 HTTP headers, payloads, and SigV4 authorization are not rewritten.
* Do not expose the BlindBucket port directly to the Internet.
* The Android side is still under integration; this server alone is not a complete installable product.
