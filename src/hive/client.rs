use crate::hive::identity::DeviceIdentity;
use anyhow::{Context, Result};

/// An outbound HTTP client for hive-to-hive requests, authenticated with
/// this device's identity (via a client cert wrapping its Ed25519 key) and
/// pinned to accept only the one specific peer named by
/// `target_public_key_hex` -- not any roster member broadly, this one peer.
pub struct HiveClient {
    inner: reqwest::Client,
}

impl HiveClient {
    /// Builds an mTLS client that presents `identity`'s self-signed cert and
    /// verifies the server's certificate's public key matches
    /// `target_public_key_hex` exactly.
    pub fn new(identity: &DeviceIdentity, target_public_key_hex: &str) -> Result<Self> {
        let target_public_key: [u8; 32] = hex::decode(target_public_key_hex)
            .context("decoding target public key hex")?
            .try_into()
            .map_err(|_| anyhow::anyhow!("target public key must be 32 bytes"))?;

        // Cached per-identity: the self-signed cert is deterministic from the
        // fixed device identity, and this constructor runs per online peer on
        // every memory write (push-on-change), so recomputing it each time
        // would be pure waste. See `cert::self_signed_cert_der`.
        let (client_cert_der, client_key_der) =
            crate::hive::cert::self_signed_cert_der(identity)?;
        let client_cert_der = rustls::pki_types::CertificateDer::from(client_cert_der);

        let verifier = std::sync::Arc::new(crate::hive::tls_verify::PinnedServerCertVerifier::new(
            target_public_key,
        ));

        // Explicit provider selection (not the bare `builder()`), mirroring
        // the server-side fix in `http.rs`: both `ring` and `aws-lc-rs` end
        // up compiled in (via axum-server's and reqwest's respective rustls
        // dependencies), so the bare `ClientConfig::builder()` panics at
        // runtime when it can't unambiguously pick a process-default
        // between the two crypto backends.
        let tls_config = rustls::ClientConfig::builder_with_provider(std::sync::Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_protocol_versions(rustls::DEFAULT_VERSIONS)
        .map_err(|e| anyhow::anyhow!("failed to select TLS protocol versions: {e}"))?
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_client_auth_cert(
            vec![client_cert_der],
            rustls::pki_types::PrivateKeyDer::try_from(client_key_der)
                .map_err(|e| anyhow::anyhow!("invalid client key DER: {e}"))?,
        )
        .context("building client-auth TLS config")?;

        let inner = reqwest::Client::builder()
            .use_preconfigured_tls(tls_config)
            .build()
            .context("building reqwest client with pinned mTLS config")?;

        Ok(Self { inner })
    }

    pub async fn get(&self, url: &str) -> Result<reqwest::Response> {
        Ok(self.inner.get(url).send().await?)
    }

    pub async fn post_json(&self, url: &str, body: &impl serde::Serialize) -> Result<reqwest::Response> {
        Ok(self.inner.post(url).json(body).send().await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hive::identity;

    #[test]
    fn new_client_succeeds_with_valid_identity_and_target_key() {
        let identity = identity::generate();
        let target = identity::generate();
        let target_pk = identity::public_key_hex(&target);
        let client = HiveClient::new(&identity, &target_pk);
        assert!(client.is_ok());
    }
}
