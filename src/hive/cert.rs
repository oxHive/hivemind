use crate::hive::identity::DeviceIdentity;
use anyhow::{Context, Result};
use ed25519_dalek::pkcs8::EncodePrivateKey;

/// Converts this device's existing Ed25519 signing key into an `rcgen`
/// keypair, so the TLS certificate is a wrapper around the same identity
/// used for roster join/revocation records — not a second, unrelated key.
pub fn identity_to_rcgen_keypair(identity: &DeviceIdentity) -> Result<rcgen::KeyPair> {
    let pkcs8_der = identity
        .signing_key
        .to_pkcs8_der()
        .context("encoding device signing key as PKCS#8")?;
    let pkcs8_der = rustls_pki_types::PrivatePkcs8KeyDer::from(pkcs8_der.as_bytes().to_vec());
    rcgen::KeyPair::from_pkcs8_der_and_sign_algo(&pkcs8_der, &rcgen::PKCS_ED25519)
        .context("building rcgen KeyPair from device signing key")
}

/// A self-signed certificate for this device, using its own persisted
/// identity key. Regenerated fresh each call (cheap — no network I/O),
/// since only the *public key* inside it needs to be stable across
/// restarts, and it always is, because it's derived from the persisted
/// signing key every time.
pub fn self_signed_cert(identity: &DeviceIdentity) -> Result<rcgen::CertifiedKey<rcgen::KeyPair>> {
    let keypair = identity_to_rcgen_keypair(identity)?;
    let params = rcgen::CertificateParams::new(vec![identity.device_id.clone()])
        .context("building certificate params")?;
    let cert = params
        .self_signed(&keypair)
        .context("self-signing certificate")?;
    Ok(rcgen::CertifiedKey { cert, signing_key: keypair })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hive::identity;

    #[test]
    fn cert_public_key_matches_identity_public_key() {
        let device_identity = identity::generate();
        let cert = self_signed_cert(&device_identity).unwrap();
        let der = cert.cert.der();
        let (_, parsed) = x509_parser::parse_x509_certificate(der).unwrap();
        let raw_pubkey = parsed.public_key().subject_public_key.data.as_ref();
        let expected = hex::decode(identity::public_key_hex(&device_identity)).unwrap();
        assert_eq!(raw_pubkey, expected.as_slice());
    }
}
