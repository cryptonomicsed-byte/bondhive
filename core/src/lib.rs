//! Bondhive core — NIP-01 / NIP-74 event model.
//!
//! Implements the Nostr event primitive exactly as specified by NIP-01:
//! canonical serialization rules, event id (sha256), and BIP-340 Schnorr
//! signatures over secp256k1. On top of that it builds the NIP-74 "Agent
//! Bonds and Attestations" event kinds (37000–37005) that make up the
//! Bondhive reliability layer for the agent economy.
//!
//! This crate is the single source of truth for event construction and
//! verification across the Bondhive stack (Solana program, TypeScript MCP
//! server, oracle agent). No crypto is hand-rolled: signing/verification
//! delegates to libsecp256k1 via the `secp256k1` crate.

use secp256k1::schnorr::Signature;
use secp256k1::{Keypair, Message, Secp256k1, SecretKey, XOnlyPublicKey};
use sha2::{Digest, Sha256};

/// Standard NIP-74 kinds (see spec/NIP-74.md).
pub mod nip74 {
    /// Service Bond — addressable, `d` tag = bond id. The promise.
    pub const KIND_SERVICE_BOND: u64 = 37_000;
    /// Bond Acceptance — the agent commits and collateral is staked on-chain.
    pub const KIND_BOND_ACCEPTANCE: u64 = 37_001;
    /// Performance Attestation — a witness records pass/fail.
    pub const KIND_PERFORMANCE_ATTESTATION: u64 = 37_002;
    /// Bond Claim (dispute) — gift-wrapped inner event (NIP-59 transport).
    pub const KIND_BOND_CLAIM: u64 = 37_003;
    /// Arbitration Verdict — resolves a dispute.
    pub const KIND_ARBITRATION_VERDICT: u64 = 37_004;
    /// Bond Cancellation — only valid before acceptance.
    pub const KIND_BOND_CANCELLATION: u64 = 37_005;
}

/// BondScore reputation math (no Nostr/Solana dependencies — host-testable).
pub mod score;

/// A Nostr event as defined by NIP-01.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Event {
    pub id: String,
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u64,
    pub tags: Vec<Vec<String>>,
    pub content: String,
    pub sig: String,
}

/// The unsigned shell of an event — what gets canonicalized and hashed.
#[derive(Debug, Clone, PartialEq)]
pub struct UnsignedEvent {
    pub pubkey: String,
    pub created_at: u64,
    pub kind: u64,
    pub tags: Vec<Vec<String>>,
    pub content: String,
}

impl UnsignedEvent {
    pub fn new(
        pubkey: String,
        created_at: u64,
        kind: u64,
        tags: Vec<Vec<String>>,
        content: String,
    ) -> Self {
        Self {
            pubkey,
            created_at,
            kind,
            tags,
            content,
        }
    }

    /// NIP-01 canonical serialization.
    ///
    /// Rules enforced:
    /// - UTF-8, no whitespace or line breaks
    /// - content escapes exactly: `\n \r \t \" \\ \b \f`; everything else verbatim
    /// - tags serialized compactly (serde_json emits no spaces)
    ///
    /// The serialized form is `[0,<pubkey>,<created_at>,<kind>,<tags>,<content>]`.
    pub fn canonical_json(&self) -> String {
        let tags_json = serde_json::to_string(&self.tags).expect("tags are always serializable");
        format!(
            "[0,\"{}\",{},{},{},\"{}\"]",
            self.pubkey,
            self.created_at,
            self.kind,
            tags_json,
            escape_content(&self.content)
        )
    }

    /// The event id: `sha256(canonical_json)`.
    pub fn id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(self.canonical_json().as_bytes());
        hex::encode(hasher.finalize())
    }

    /// BIP-340 Schnorr signature over the event id (32-byte digest), as NIP-01 requires.
    pub fn sign(&self, secret_key: &SecretKey) -> Result<String, secp256k1::Error> {
        let secp = Secp256k1::new();
        let keypair = Keypair::from_secret_key(&secp, secret_key);
        let id_bytes = hex::decode(self.id()).expect("sha256 output is valid hex");
        let msg = Message::from_digest_slice(&id_bytes)?;
        let mut rng = secp256k1::rand::thread_rng();
        let sig = secp.sign_schnorr_with_rng(&msg, &keypair, &mut rng);
        Ok(hex::encode(sig.serialize()))
    }

    /// Produce a fully signed event.
    pub fn into_event(self, secret_key: &SecretKey) -> Result<Event, secp256k1::Error> {
        let sig = self.sign(secret_key)?;
        Ok(Event {
            id: self.id(),
            pubkey: self.pubkey,
            created_at: self.created_at,
            kind: self.kind,
            tags: self.tags,
            content: self.content,
            sig,
        })
    }
}

impl Event {
    /// Verify the event: id must hash to the canonical serialization and the
    /// Schnorr signature must verify against the id with the claimed pubkey.
    pub fn verify(&self) -> Result<(), EventVerifyError> {
        let unsigned = UnsignedEvent::new(
            self.pubkey.clone(),
            self.created_at,
            self.kind,
            self.tags.clone(),
            self.content.clone(),
        );
        if unsigned.id() != self.id {
            return Err(EventVerifyError::BadId);
        }
        let id_bytes = hex::decode(&self.id).map_err(|_| EventVerifyError::MalformedHex)?;
        let sig_bytes = hex::decode(&self.sig).map_err(|_| EventVerifyError::MalformedHex)?;
        let pubkey_bytes = hex::decode(&self.pubkey).map_err(|_| EventVerifyError::MalformedHex)?;
        let msg =
            Message::from_digest_slice(&id_bytes).map_err(|_| EventVerifyError::MalformedId)?;
        let sig = Signature::from_slice(&sig_bytes).map_err(|_| EventVerifyError::MalformedSig)?;
        let xonly = XOnlyPublicKey::from_slice(&pubkey_bytes)
            .map_err(|_| EventVerifyError::MalformedPubkey)?;
        let secp = Secp256k1::new();
        secp.verify_schnorr(&sig, &msg, &xonly)
            .map_err(|_| EventVerifyError::BadSignature)
    }

    pub fn kind(&self) -> u64 {
        self.kind
    }

    /// First value of the first tag with the given name.
    pub fn tag(&self, name: &str) -> Option<&str> {
        self.tags
            .iter()
            .find(|t| t.first().map(String::as_str) == Some(name))
            .and_then(|t| t.get(1))
            .map(String::as_str)
    }

    /// All values of tags with the given name.
    pub fn tag_values(&self, name: &str) -> Vec<&str> {
        self.tags
            .iter()
            .filter(|t| t.first().map(String::as_str) == Some(name))
            .filter_map(|t| t.get(1).map(String::as_str))
            .collect()
    }
}

/// Reasons an event fails verification.
#[derive(Debug, Clone, PartialEq)]
pub enum EventVerifyError {
    BadId,
    MalformedHex,
    MalformedId,
    MalformedSig,
    MalformedPubkey,
    BadSignature,
}

impl std::fmt::Display for EventVerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// NIP-01 content escaping — exactly the six escapes the spec mandates.
fn escape_content(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{0008}' => out.push_str("\\b"),
            '\u{000C}' => out.push_str("\\f"),
            c => out.push(c),
        }
    }
    out
}

/// Generate a fresh Nostr keypair (32-byte secret key + x-only public key hex).
pub fn generate_keypair() -> (String, String) {
    let mut rng = secp256k1::rand::rngs::OsRng;
    let secp = Secp256k1::new();
    let (sk, _pk) = secp.generate_keypair(&mut rng);
    let keypair = Keypair::from_secret_key(&secp, &sk);
    let (xonly, _) = XOnlyPublicKey::from_keypair(&keypair);
    (
        hex::encode(sk.secret_bytes()),
        hex::encode(xonly.serialize()),
    )
}

// ---------------------------------------------------------------------------
// NIP-74 event builders
// ---------------------------------------------------------------------------

/// Helpers to build NIP-74 bond events.
pub mod builders {
    use super::*;

    /// Build a kind 37000 Service Bond (the promise).
    ///
    /// `d` tag = bond id (32-hex task hash). Tags carry the agent (`p`), the
    /// Buzz channel (`h`, NIP-29 group id), witnesses, task type, amount,
    /// asset, chain and deadline. Content is a JSON description.
    #[allow(clippy::too_many_arguments)]
    pub fn service_bond(
        agent_pubkey: &str,
        channel: &str,
        witnesses: &[String],
        task_type: &str,
        bond_id: &str,
        amount: &str,
        asset: &str,
        deadline: u64,
        created_at: u64,
        description: &str,
    ) -> UnsignedEvent {
        let mut tags: Vec<Vec<String>> = vec![
            vec!["d".into(), bond_id.into()],
            vec!["p".into(), agent_pubkey.into()],
            vec!["h".into(), channel.into()],
            vec!["t".into(), task_type.into()],
            vec!["amount".into(), amount.into()],
            vec!["asset".into(), asset.into()],
            vec!["chain".into(), "solana".into()],
            vec!["deadline".into(), deadline.to_string()],
        ];
        for w in witnesses {
            tags.push(vec!["witness".into(), w.clone()]);
        }
        let content = serde_json::json!({
            "task": task_type,
            "description": description,
            "amount": amount,
            "asset": asset,
            "chain": "solana",
            "deadline": deadline,
            "created_by_role": "agent",
        })
        .to_string();
        UnsignedEvent::new(
            agent_pubkey.to_string(),
            created_at,
            nip74::KIND_SERVICE_BOND,
            tags,
            content,
        )
    }

    /// Kind 37001 Bond Acceptance — references the bond via an `a` tag
    /// (`37000:<agent pubkey>:<bond id>`), reaffirms the roster and amount.
    pub fn acceptance(
        agent_pubkey: &str,
        bond_address: &str,
        amount: &str,
        witnesses: &[String],
        created_at: u64,
    ) -> UnsignedEvent {
        let mut tags: Vec<Vec<String>> = vec![
            vec!["a".into(), bond_address.into()],
            vec!["p".into(), agent_pubkey.into()],
            vec!["amount".into(), amount.into()],
        ];
        for w in witnesses {
            tags.push(vec!["witness".into(), w.clone()]);
        }
        UnsignedEvent::new(
            agent_pubkey.to_string(),
            created_at,
            nip74::KIND_BOND_ACCEPTANCE,
            tags,
            "I bind this service bond and stake collateral.".into(),
        )
    }

    /// Kind 37002 Performance Attestation — `v` = `pass` | `fail`,
    /// optional `evidence` reference (artifact hash or URL).
    pub fn attestation(
        witness_pubkey: &str,
        bond_address: &str,
        verdict: &str,
        evidence: Option<&str>,
        created_at: u64,
    ) -> UnsignedEvent {
        let mut tags: Vec<Vec<String>> = vec![
            vec!["a".into(), bond_address.into()],
            vec!["p".into(), witness_pubkey.into()],
            vec!["v".into(), verdict.into()],
        ];
        if let Some(ev) = evidence {
            tags.push(vec!["evidence".into(), ev.into()]);
        }
        UnsignedEvent::new(
            witness_pubkey.to_string(),
            created_at,
            nip74::KIND_PERFORMANCE_ATTESTATION,
            tags,
            format!("Attestation: {verdict}"),
        )
    }

    /// Kind 37003 Bond Claim (dispute). Transport MUST be NIP-59 gift wrap;
    /// this builder produces the inner event.
    pub fn claim(
        claimant_pubkey: &str,
        bond_address: &str,
        amount: &str,
        reason: &str,
        created_at: u64,
    ) -> UnsignedEvent {
        let tags = vec![
            vec!["a".into(), bond_address.into()],
            vec!["p".into(), claimant_pubkey.into()],
            vec!["amount".into(), amount.into()],
        ];
        let content = serde_json::json!({ "reason": reason }).to_string();
        UnsignedEvent::new(
            claimant_pubkey.to_string(),
            created_at,
            nip74::KIND_BOND_CLAIM,
            tags,
            content,
        )
    }

    /// Kind 37004 Arbitration Verdict — `v` = `uphold` | `reject` | `split`.
    pub fn verdict(
        arbitrator_pubkey: &str,
        bond_address: &str,
        verdict: &str,
        split_ratio: Option<u8>,
        rationale: &str,
        created_at: u64,
    ) -> UnsignedEvent {
        let mut tags = vec![
            vec!["a".into(), bond_address.into()],
            vec!["p".into(), arbitrator_pubkey.into()],
            vec!["v".into(), verdict.into()],
        ];
        if let Some(ratio) = split_ratio {
            tags.push(vec!["split".into(), ratio.to_string()]);
        }
        let content = serde_json::json!({ "rationale": rationale }).to_string();
        UnsignedEvent::new(
            arbitrator_pubkey.to_string(),
            created_at,
            nip74::KIND_ARBITRATION_VERDICT,
            tags,
            content,
        )
    }

    /// Kind 37005 Bond Cancellation — only valid before acceptance.
    pub fn cancellation(
        agent_pubkey: &str,
        bond_address: &str,
        reason: &str,
        created_at: u64,
    ) -> UnsignedEvent {
        let tags = vec![
            vec!["a".into(), bond_address.into()],
            vec!["p".into(), agent_pubkey.into()],
            vec!["reason".into(), reason.into()],
        ];
        UnsignedEvent::new(
            agent_pubkey.to_string(),
            created_at,
            nip74::KIND_BOND_CANCELLATION,
            tags,
            reason.into(),
        )
    }

    /// NIP-32 label event (kind 1985) publishing a BondScore under the
    /// `bondhive` namespace. `L` declares the namespace, `l` the metric.
    pub fn bondscore_label(
        publisher_pubkey: &str,
        subject_pubkey: &str,
        metric: &str,
        value: &str,
        created_at: u64,
    ) -> UnsignedEvent {
        let tags = vec![
            vec!["L".into(), "bondhive".into()],
            vec!["l".into(), metric.into(), subject_pubkey.into()],
            vec!["p".into(), subject_pubkey.into()],
        ];
        UnsignedEvent::new(
            publisher_pubkey.to_string(),
            created_at,
            1985,
            tags,
            value.into(),
        )
    }

    /// NIP-89 handler announcement (kind 31990) for the Bondhive oracle app.
    pub fn handler_announcement(
        app_pubkey: &str,
        name: &str,
        url: &str,
        handled_kinds: &[u64],
        created_at: u64,
    ) -> UnsignedEvent {
        let mut tags: Vec<Vec<String>> = vec![
            vec!["d".into(), "bondhive-oracle".into()],
            vec!["name".into(), name.into()],
            vec!["url".into(), url.into()],
        ];
        for k in handled_kinds {
            tags.push(vec!["k".into(), k.to_string()]);
        }
        UnsignedEvent::new(
            app_pubkey.to_string(),
            created_at,
            31_990,
            tags,
            String::new(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escape_content_matches_nip01() {
        let input = "line1\nline2\t\"quoted\"\\backslash \u{0008}\u{000C} done";
        let expected = "line1\\nline2\\t\\\"quoted\\\"\\\\backslash \\b\\f done";
        assert_eq!(escape_content(input), expected);
    }
}
