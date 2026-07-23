use crate::hive::roster::{RosterEntry, RosterStatus};
use anyhow::{bail, Context, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use std::fmt;

/// Extracts the raw 32-byte Ed25519 public key from an X.509 certificate's
/// SubjectPublicKeyInfo. For Ed25519 (RFC 8410), the SPKI's raw bit-string
/// content *is* the 32-byte public key with no algorithm parameters mixed
/// in, so no further decoding is needed once x509-parser hands back the
/// bit-string bytes.
pub fn extract_ed25519_public_key(cert_der: &[u8]) -> Result<[u8; 32]> {
    let (_, parsed) =
        x509_parser::parse_x509_certificate(cert_der).context("parsing peer certificate")?;
    let raw = parsed.public_key().subject_public_key.data.as_ref();
    if raw.len() != 32 {
        bail!("expected a 32-byte Ed25519 public key, got {} bytes", raw.len());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(raw);
    Ok(out)
}

/// Accepts a client certificate only if its public key matches an `Active`
/// entry in the given roster snapshot. The roster snapshot is passed in at
/// construction time (fetched fresh before each TLS listener rebuild, or
/// periodically refreshed — wiring that refresh is a later task's job, not
/// this one; this type just needs a roster to check against).
pub struct RosterClientCertVerifier {
    roster: Vec<RosterEntry>,
}

impl fmt::Debug for RosterClientCertVerifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RosterClientCertVerifier")
            .field(
                "active_members",
                &self.roster.iter().filter(|e| e.status == RosterStatus::Active).count(),
            )
            .finish()
    }
}

impl RosterClientCertVerifier {
    pub fn new(roster: Vec<RosterEntry>) -> Self {
        Self { roster }
    }
}

impl ClientCertVerifier for RosterClientCertVerifier {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }

    fn verify_client_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _now: UnixTime,
    ) -> Result<ClientCertVerified, rustls::Error> {
        let Ok(public_key) = extract_ed25519_public_key(end_entity) else {
            return Err(rustls::Error::General(
                "could not extract Ed25519 public key from client certificate".into(),
            ));
        };
        let public_key_hex = hex::encode(public_key);
        let is_active_member = self
            .roster
            .iter()
            .any(|e| e.status == RosterStatus::Active && e.public_key == public_key_hex);
        if is_active_member {
            Ok(ClientCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "client certificate's public key is not an Active hive roster member".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// Accepts a server's TLS certificate only if its public key matches one
/// specific expected device — used when this device initiates an outbound
/// hive call to a *known* peer (looked up from the local roster before
/// connecting). Unlike `RosterClientCertVerifier` (which accepts any
/// currently-Active member), this is a tight per-connection pin: exactly
/// one acceptable public key, not "any roster member."
#[derive(Debug)]
pub struct PinnedServerCertVerifier {
    expected_public_key: [u8; 32],
}

impl PinnedServerCertVerifier {
    pub fn new(expected_public_key: [u8; 32]) -> Self {
        Self { expected_public_key }
    }
}

impl ServerCertVerifier for PinnedServerCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let Ok(public_key) = extract_ed25519_public_key(end_entity) else {
            return Err(rustls::Error::General(
                "could not extract Ed25519 public key from server certificate".into(),
            ));
        };
        if public_key == self.expected_public_key {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(
                "server certificate's public key does not match the expected hive peer".into(),
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &rustls::crypto::ring::default_provider().signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        rustls::crypto::ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hive::{cert, identity};

    #[test]
    fn extract_ed25519_public_key_matches_identity() {
        let device_identity = identity::generate();
        let certified = cert::self_signed_cert(&device_identity).unwrap();
        let der = certified.cert.der();
        let extracted = extract_ed25519_public_key(der).unwrap();
        let expected = hex::decode(identity::public_key_hex(&device_identity)).unwrap();
        assert_eq!(extracted.to_vec(), expected);
    }

    #[test]
    fn extract_ed25519_public_key_rejects_garbage_der() {
        assert!(extract_ed25519_public_key(b"not a certificate").is_err());
    }

    #[test]
    fn pinned_verifier_accepts_matching_key_rejects_mismatched() {
        let a = identity::generate();
        let b = identity::generate();
        let cert_a = cert::self_signed_cert(&a).unwrap();
        let der_a = cert_a.cert.der();
        let pubkey_a = extract_ed25519_public_key(der_a).unwrap();
        let pubkey_b_hex = identity::public_key_hex(&b);
        let pubkey_b: [u8; 32] = hex::decode(&pubkey_b_hex).unwrap().try_into().unwrap();

        let verifier_expects_a = PinnedServerCertVerifier::new(pubkey_a);
        let verifier_expects_b = PinnedServerCertVerifier::new(pubkey_b);

        let now = rustls::pki_types::UnixTime::now();
        let cert_der = rustls::pki_types::CertificateDer::from(der_a.to_vec());
        let server_name = rustls::pki_types::ServerName::try_from(a.device_id.clone()).unwrap();

        assert!(verifier_expects_a
            .verify_server_cert(&cert_der, &[], &server_name, &[], now)
            .is_ok());
        assert!(verifier_expects_b
            .verify_server_cert(&cert_der, &[], &server_name, &[], now)
            .is_err());
    }
}
