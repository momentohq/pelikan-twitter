// Copyright 2020 Twitter, Inc.
// Licensed under the Apache License, Version 2.0
// http://www.apache.org/licenses/LICENSE-2.0

use config::PingserverConfig;
use openssl::ssl::SslAcceptor;
use openssl::ssl::SslContext;
use openssl::ssl::SslFiletype;
use openssl::ssl::SslMethod;

use std::sync::Arc;

pub enum Message {
    Shutdown,
}

pub fn ssl_context(config: &Arc<PingserverConfig>) -> Result<Option<SslContext>, std::io::Error> {
    let mut builder = SslAcceptor::mozilla_intermediate_v5(SslMethod::tls_server())?;

    if let Some(f) = config.tls().certificate_chain() {
        builder.set_ca_file(f);
    } else {
        return Ok(None);
    }

    if let Some(f) = config.tls().certificate() {
        builder.set_certificate_file(f, SslFiletype::PEM);
    } else {
        return Ok(None);
    }

    if let Some(f) = config.tls().private_key() {
        builder.set_private_key_file(f, SslFiletype::PEM);
    } else {
        return Ok(None);
    }

    Ok(Some(builder.build().into_context()))
}

pub fn load_tls_config(
    config: &Arc<PingserverConfig>,
) -> Result<Option<Arc<rustls::ServerConfig>>, std::io::Error> {
    let verifier = if let Some(certificate_chain) = config.tls().certificate_chain() {
        let mut certstore = rustls::RootCertStore::empty();
        let cafile = std::fs::File::open(certificate_chain).map_err(|e| {
            error!("{}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Could not open CA file")
        })?;
        certstore
            .add_pem_file(&mut std::io::BufReader::new(cafile))
            .map_err(|_| {
                std::io::Error::new(std::io::ErrorKind::Other, "Could not parse CA file")
            })?;
        Some(rustls::AllowAnyAnonymousOrAuthenticatedClient::new(
            certstore,
        ))
    } else {
        None
    };

    let cert = if let Some(certificate) = config.tls().certificate() {
        let certfile = std::fs::File::open(certificate).map_err(|e| {
            error!("{}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Could not open certificate file")
        })?;
        Some(
            rustls::internal::pemfile::certs(&mut std::io::BufReader::new(certfile)).map_err(
                |_| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Could not parse certificate file",
                    )
                },
            )?,
        )
    } else {
        None
    };

    let key = if let Some(private_key) = config.tls().private_key() {
        let keyfile = std::fs::File::open(private_key).map_err(|e| {
            error!("{}", e);
            std::io::Error::new(std::io::ErrorKind::Other, "Could not open private key file")
        })?;
        let keys =
            rustls::internal::pemfile::pkcs8_private_keys(&mut std::io::BufReader::new(keyfile))
                .map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "Could not parse private key file",
                    )
                })?;
        if keys.len() != 1 {
            fatal!("Expected 1 private key, got: {}", keys.len());
        }
        Some(keys[0].clone())
    } else {
        None
    };

    if verifier.is_none() && cert.is_none() && key.is_none() {
        Ok(None)
    } else if verifier.is_some() && cert.is_some() && key.is_some() {
        let mut tls_config = rustls::ServerConfig::new(verifier.unwrap());
        let _ = tls_config.set_single_cert(cert.unwrap(), key.unwrap());
        Ok(Some(Arc::new(tls_config)))
    } else {
        error!("Incomplete TLS config");
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "Incomplete TLS config",
        ))
    }
}
