use agentdock_core::remote::{
    RemoteAuthChallenge, RemoteAuthProof, REMOTE_PROTOCOL_VERSION,
};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use thiserror::Error;

const ED25519_PREFIX: &str = "ed25519:";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RemoteAuthError {
    #[error("unsupported remote protocol version")]
    ProtocolVersion,
    #[error("remote authentication proof does not match the challenge")]
    ChallengeBinding,
    #[error("invalid remote authentication nonce")]
    InvalidNonce,
    #[error("paired device public key must be ed25519:<64 hex characters>")]
    InvalidPublicKey,
    #[error("remote authentication signature must be ed25519:<128 hex characters>")]
    InvalidSignature,
    #[error("remote authentication signature verification failed")]
    SignatureVerification,
}

pub fn verify_device_proof(
    public_key: &str,
    challenge: &RemoteAuthChallenge,
    proof: &RemoteAuthProof,
) -> Result<(), RemoteAuthError> {
    let transcript = auth_transcript(challenge, proof)?;
    let verifying_key = parse_verifying_key(public_key)?;
    let signature = parse_signature(&proof.signature)?;

    verifying_key
        .verify(transcript.as_bytes(), &signature)
        .map_err(|_| RemoteAuthError::SignatureVerification)
}

pub fn auth_transcript(
    challenge: &RemoteAuthChallenge,
    proof: &RemoteAuthProof,
) -> Result<String, RemoteAuthError> {
    if challenge.protocol_version != REMOTE_PROTOCOL_VERSION
        || proof.protocol_version != REMOTE_PROTOCOL_VERSION
    {
        return Err(RemoteAuthError::ProtocolVersion);
    }

    if challenge.device_id != proof.device_id || challenge.challenge_id != proof.challenge_id {
        return Err(RemoteAuthError::ChallengeBinding);
    }

    if !valid_nonce(&challenge.server_nonce) || !valid_nonce(&proof.client_nonce) {
        return Err(RemoteAuthError::InvalidNonce);
    }

    Ok(format!(
        "agentdock-remote-auth-v1\nprotocol_version={}\ndevice_id={}\nchallenge_id={}\nserver_nonce={}\nclient_nonce={}\n",
        REMOTE_PROTOCOL_VERSION,
        challenge.device_id,
        challenge.challenge_id,
        challenge.server_nonce,
        proof.client_nonce
    ))
}

pub fn is_supported_device_public_key(value: &str) -> bool {
    parse_verifying_key(value).is_ok()
}

fn parse_verifying_key(value: &str) -> Result<VerifyingKey, RemoteAuthError> {
    let hex = value
        .strip_prefix(ED25519_PREFIX)
        .ok_or(RemoteAuthError::InvalidPublicKey)?;
    let bytes = decode_hex_array::<32>(hex).ok_or(RemoteAuthError::InvalidPublicKey)?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| RemoteAuthError::InvalidPublicKey)
}

fn parse_signature(value: &str) -> Result<Signature, RemoteAuthError> {
    let hex = value
        .strip_prefix(ED25519_PREFIX)
        .ok_or(RemoteAuthError::InvalidSignature)?;
    let bytes = decode_hex_array::<64>(hex).ok_or(RemoteAuthError::InvalidSignature)?;
    Ok(Signature::from_bytes(&bytes))
}

fn valid_nonce(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn decode_hex_array<const N: usize>(value: &str) -> Option<[u8; N]> {
    if value.len() != N * 2 {
        return None;
    }

    let mut output = [0_u8; N];
    for (index, slot) in output.iter_mut().enumerate() {
        let offset = index * 2;
        *slot = u8::from_str_radix(&value[offset..offset + 2], 16).ok()?;
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn challenge(public_key_device_id: &str) -> RemoteAuthChallenge {
        RemoteAuthChallenge {
            protocol_version: REMOTE_PROTOCOL_VERSION,
            device_id: public_key_device_id.to_string(),
            challenge_id: "rac_test".into(),
            server_nonce: "11".repeat(32),
            issued_at_ms: 1_000,
            expires_at_ms: 10_000,
        }
    }

    fn public_key(signing_key: &SigningKey) -> String {
        format!(
            "{ED25519_PREFIX}{}",
            signing_key
                .verifying_key()
                .to_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        )
    }

    #[test]
    fn verifies_bound_ed25519_proof() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let challenge = challenge("dev_test");
        let mut proof = RemoteAuthProof {
            protocol_version: REMOTE_PROTOCOL_VERSION,
            device_id: challenge.device_id.clone(),
            challenge_id: challenge.challenge_id.clone(),
            client_nonce: "22".repeat(32),
            signature: String::new(),
        };

        let transcript = auth_transcript(&challenge, &proof).unwrap();
        let signature = signing_key.sign(transcript.as_bytes());
        proof.signature = format!(
            "{ED25519_PREFIX}{}",
            signature
                .to_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );

        assert_eq!(
            verify_device_proof(&public_key(&signing_key), &challenge, &proof),
            Ok(())
        );
    }

    #[test]
    fn rejects_parameter_substitution_in_auth_transcript() {
        let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
        let challenge = challenge("dev_test");
        let mut proof = RemoteAuthProof {
            protocol_version: REMOTE_PROTOCOL_VERSION,
            device_id: challenge.device_id.clone(),
            challenge_id: challenge.challenge_id.clone(),
            client_nonce: "33".repeat(32),
            signature: String::new(),
        };

        let transcript = auth_transcript(&challenge, &proof).unwrap();
        let signature = signing_key.sign(transcript.as_bytes());
        proof.signature = format!(
            "{ED25519_PREFIX}{}",
            signature
                .to_bytes()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );

        proof.client_nonce = "44".repeat(32);
        assert_eq!(
            verify_device_proof(&public_key(&signing_key), &challenge, &proof),
            Err(RemoteAuthError::SignatureVerification)
        );
    }

    #[test]
    fn rejects_cross_device_proof() {
        let challenge = challenge("dev_one");
        let proof = RemoteAuthProof {
            protocol_version: REMOTE_PROTOCOL_VERSION,
            device_id: "dev_two".into(),
            challenge_id: challenge.challenge_id.clone(),
            client_nonce: "55".repeat(32),
            signature: format!("{ED25519_PREFIX}{}", "00".repeat(64)),
        };

        assert_eq!(
            auth_transcript(&challenge, &proof),
            Err(RemoteAuthError::ChallengeBinding)
        );
    }
}
