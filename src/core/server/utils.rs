use std::{ffi::OsStr, io::BufReader};
use std::sync::Arc;

use rustls_pemfile::{certs, pkcs8_private_keys};
use tokio_rustls::TlsAcceptor;

use crate::racoon_info;

pub fn tls_acceptor_from_path<S: AsRef<OsStr>>(
    certificate_path: S,
    private_key_path: S,
) -> std::io::Result<TlsAcceptor> {
    // Tries to read certificate file
    let certificate_file = match std::fs::File::open(certificate_path.as_ref()) {
        Ok(file) => file,
        Err(error) => {
            return Err(std::io::Error::other(format!(
                "Failed to open certificate file. Error: {}",
                error
            )));
        }
    };

    let mut certificate_buffered_reader = BufReader::new(certificate_file);

    // Extracts certificates
    let mut certificates = vec![];
    for certificate in certs(&mut certificate_buffered_reader) {
        certificates.push(certificate?);
    }

    racoon_info!("Found certificates: {}", certificates.len());

    // Tries to read private key file
    let private_key_file = match std::fs::File::open(private_key_path.as_ref()) {
        Ok(file) => file,
        Err(error) => {
            return Err(std::io::Error::other(format!(
                "Failed to open private key file. Error: {}",
                error
            )));
        }
    };

    let mut private_key_buffered_reader = BufReader::new(private_key_file);

    // Extracts private key
    let key_options = pkcs8_private_keys(&mut private_key_buffered_reader).next();
    if let Some(key) = key_options {
        let private_key_der = rustls::pki_types::PrivateKeyDer::Pkcs8(key?);
        let server_config_result = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certificates, private_key_der);

        let server_config = match server_config_result {
            Ok(config) => config,
            Err(error) => {
                return Err(std::io::Error::other(format!(
                    "Failed to create server configuraiton. Error: {}",
                    error
                )));
            }
        };

        return Ok(TlsAcceptor::from(Arc::new(server_config)));
    } else {
        return Err(std::io::Error::other("Private key not found."));
    }
}
