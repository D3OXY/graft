//! End-to-end encrypted environment primitives.
//!
//! The module deliberately delegates encryption framing to native age files. Graft owns
//! only routing, canonical transcripts, and the signed Keyring transition.

use std::{
    collections::BTreeMap,
    fmt, fs,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    str::FromStr,
};

use age::{
    secrecy::ExposeSecret, x25519, Decryptor, Encryptor, Identity as AgeIdentity,
    Recipient as AgeRecipient,
};
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use zeroize::Zeroizing;

use crate::{artifact::ArtifactError, Artifact, ArtifactId, ArtifactKind, RepoStore};

const CREDENTIAL_MAGIC: &[u8] = b"graft-credential-v1\0";
const KEYRING_MAGIC: &[u8] = b"keyring-v1\0";
const BUNDLE_MAGIC: &[u8] = b"secret-bundle-v1\0";
const ENV_IDENTITY_MAGIC: &[u8] = b"graft-env-identity-v1\0";
const HISTORY_MAGIC: &[u8] = b"graft-env-history-link-v1\0";
const MEMBER_LABEL: &[u8] = b"graft/member/v1\0";
const KEYRING_SIGNATURE_LABEL: &[u8] = b"graft/keyring-signature/v1\0";
const MAX_BUNDLE_BYTES: usize = 1024 * 1024;
const MAX_VALUE_BYTES: usize = 64 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("credential file is missing or unreadable")]
    MissingCredential,
    #[error("credential file is malformed")]
    MalformedCredential,
    #[error("credential file could not be written: {0}")]
    CredentialIo(#[source] std::io::Error),
    #[error("age encryption failed")]
    AgeEncrypt,
    #[error("age ciphertext is invalid or cannot be decrypted")]
    AgeDecrypt,
    #[error("canonical secret data is malformed")]
    MalformedCanonical,
    #[error("secret name `{0}` is invalid")]
    InvalidName(String),
    #[error("secret value is not valid portable UTF-8")]
    InvalidValue,
    #[error("secret value exceeds the 64 KiB limit")]
    ValueTooLarge,
    #[error("secret bundle exceeds the 1 MiB limit")]
    BundleTooLarge,
    #[error("invalid dotenv syntax at line {0}")]
    MalformedDotenvLine(usize),
    #[error("environment name is invalid")]
    InvalidEnvironment,
    #[error("keyring signature is invalid")]
    InvalidSignature,
    #[error("keyring contains an invalid public key")]
    InvalidPublicKey,
    #[error("keyring predecessor is invalid")]
    InvalidPredecessor,
    #[error("keyring member identity does not match its fingerprint")]
    InvalidMember,
    #[error("artifact is not a secret artifact")]
    WrongArtifactKind,
    #[error("secret routing metadata does not match")]
    WrongRouting,
    #[error("environment `{0}` is not present in the keyring")]
    MissingEnvironment(String),
    #[error("repository error: {0}")]
    Store(#[from] crate::store::StoreError),
    #[error("artifact error: {0}")]
    Artifact(#[from] ArtifactError),
}

#[derive(Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MemberId([u8; 32]);

impl MemberId {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for MemberId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("MemberId")
            .field(&self.to_string())
            .finish()
    }
}

impl fmt::Display for MemberId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl FromStr for MemberId {
    type Err = SecretError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.len() != 64 {
            return Err(SecretError::MalformedCanonical);
        }
        let mut output = [0u8; 32];
        for (index, pair) in value.as_bytes().as_chunks::<2>().0.iter().enumerate() {
            output[index] = nibble(pair[0])? << 4 | nibble(pair[1])?;
        }
        Ok(Self(output))
    }
}

fn nibble(byte: u8) -> Result<u8, SecretError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(SecretError::MalformedCanonical),
    }
}

/// The one portable user credential. It intentionally has no `Debug` implementation.
pub struct Credential {
    age_identity: x25519::Identity,
    signing_key: SigningKey,
}

impl Clone for Credential {
    fn clone(&self) -> Self {
        Self {
            age_identity: self.age_identity.clone(),
            signing_key: self.signing_key.clone(),
        }
    }
}

impl Credential {
    pub fn generate() -> Self {
        let seed: [u8; 32] = rand::random();
        Self {
            age_identity: x25519::Identity::generate(),
            signing_key: SigningKey::from_bytes(&seed),
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, SecretError> {
        let bytes = fs::read(path).map_err(|_| SecretError::MissingCredential)?;
        decode_credential(&bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SecretError> {
        decode_credential(bytes)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        encode_credential(self)
    }

    pub fn save_atomic(&self, path: impl AsRef<Path>) -> Result<(), SecretError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(SecretError::CredentialIo)?;
        }
        let temporary = path.with_extension("tmp");
        let bytes = encode_credential(self);
        let write_result = (|| {
            let mut file = fs::File::create(&temporary).map_err(SecretError::CredentialIo)?;
            set_owner_only(&file).map_err(SecretError::CredentialIo)?;
            file.write_all(&bytes).map_err(SecretError::CredentialIo)?;
            file.sync_all().map_err(SecretError::CredentialIo)?;
            fs::rename(&temporary, path).map_err(SecretError::CredentialIo)
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        write_result
    }

    pub fn load_or_create(path: impl AsRef<Path>) -> Result<(Self, bool), SecretError> {
        let path = path.as_ref();
        match Self::load(path) {
            Ok(credential) => Ok((credential, false)),
            Err(SecretError::MissingCredential) => {
                let credential = Self::generate();
                credential.save_atomic(path)?;
                Ok((credential, true))
            }
            Err(error) => Err(error),
        }
    }

    pub fn default_path() -> Result<PathBuf, SecretError> {
        if let Ok(path) = std::env::var("GRAFT_CREDENTIAL_FILE") {
            return Ok(PathBuf::from(path));
        }
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .ok_or(SecretError::MissingCredential)?;
        Ok(PathBuf::from(home).join(".graft").join("credential"))
    }

    pub fn age_identity(&self) -> &x25519::Identity {
        &self.age_identity
    }

    pub fn member_identity(&self) -> MemberIdentity {
        MemberIdentity::from_credential(self)
    }

    fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }
}

fn encode_credential(credential: &Credential) -> Vec<u8> {
    let age = credential.age_identity.to_string();
    let age = age.expose_secret().as_bytes();
    let mut output = Vec::with_capacity(CREDENTIAL_MAGIC.len() + 4 + age.len() + 32);
    output.extend_from_slice(CREDENTIAL_MAGIC);
    put_u32(&mut output, age.len()).expect("age identity fits");
    output.extend_from_slice(age);
    output.extend_from_slice(&credential.signing_key.to_bytes());
    output
}

fn decode_credential(bytes: &[u8]) -> Result<Credential, SecretError> {
    let mut reader = Reader::new(bytes);
    if reader.take(CREDENTIAL_MAGIC.len())? != CREDENTIAL_MAGIC {
        return Err(SecretError::MalformedCredential);
    }
    let age = String::from_utf8(reader.bytes_u32()?.to_vec())
        .map_err(|_| SecretError::MalformedCredential)?;
    let age_identity = age.parse().map_err(|_| SecretError::MalformedCredential)?;
    let seed = reader.array_32()?;
    reader
        .finish()
        .map_err(|_| SecretError::MalformedCredential)?;
    seed_to_credential(age_identity, seed)
}

fn seed_to_credential(
    age_identity: x25519::Identity,
    seed: [u8; 32],
) -> Result<Credential, SecretError> {
    Ok(Credential {
        age_identity,
        signing_key: SigningKey::from_bytes(&seed),
    })
}

#[cfg(unix)]
fn set_owner_only(file: &fs::File) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
}

#[cfg(not(unix))]
fn set_owner_only(_file: &fs::File) -> std::io::Result<()> {
    Ok(())
}

#[derive(Clone, Eq, PartialEq)]
pub struct MemberIdentity {
    member_id: MemberId,
    age_recipient: String,
    signing_public: [u8; 32],
}

impl fmt::Debug for MemberIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MemberIdentity")
            .field("member_id", &self.member_id)
            .field("age_recipient", &self.age_recipient)
            .finish_non_exhaustive()
    }
}

impl MemberIdentity {
    pub fn new(age_recipient: String, signing_public: [u8; 32]) -> Result<Self, SecretError> {
        let _: x25519::Recipient = age_recipient
            .parse()
            .map_err(|_| SecretError::InvalidMember)?;
        VerifyingKey::from_bytes(&signing_public).map_err(|_| SecretError::InvalidPublicKey)?;
        let member_id = member_id_for(&age_recipient, &signing_public);
        Ok(Self {
            member_id,
            age_recipient,
            signing_public,
        })
    }

    fn from_credential(credential: &Credential) -> Self {
        let age = credential.age_identity.to_public().to_string();
        let age = age.to_string();
        let public = credential.signing_key.verifying_key().to_bytes();
        Self {
            member_id: member_id_for(&age, &public),
            age_recipient: age,
            signing_public: public,
        }
    }

    pub fn member_id(&self) -> MemberId {
        self.member_id
    }
    pub fn age_recipient(&self) -> &str {
        &self.age_recipient
    }
    pub fn signing_public(&self) -> [u8; 32] {
        self.signing_public
    }

    fn verify_id(&self) -> Result<(), SecretError> {
        if self.member_id != member_id_for(&self.age_recipient, &self.signing_public) {
            Err(SecretError::InvalidMember)
        } else {
            Ok(())
        }
    }
}

fn member_id_for(age_recipient: &str, signing_public: &[u8; 32]) -> MemberId {
    let mut bytes = Vec::with_capacity(MEMBER_LABEL.len() + 4 + age_recipient.len() + 32);
    bytes.extend_from_slice(MEMBER_LABEL);
    put_u32(&mut bytes, age_recipient.len()).expect("recipient fits");
    bytes.extend_from_slice(age_recipient.as_bytes());
    bytes.extend_from_slice(signing_public);
    MemberId::from_bytes(*blake3::hash(&bytes).as_bytes())
}

pub struct SecretMap {
    values: BTreeMap<String, Zeroizing<Vec<u8>>>,
}

impl SecretMap {
    pub fn empty() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    pub fn from_pairs<I>(pairs: I) -> Result<Self, SecretError>
    where
        I: IntoIterator<Item = (String, Vec<u8>)>,
    {
        let mut values: BTreeMap<String, Zeroizing<Vec<u8>>> = BTreeMap::new();
        for (name, value) in pairs {
            validate_secret_name(&name)?;
            validate_secret_value(&value)?;
            if values
                .keys()
                .any(|existing| existing.eq_ignore_ascii_case(&name) && existing != &name)
            {
                return Err(SecretError::InvalidName(name));
            }
            if values.insert(name, Zeroizing::new(value)).is_some() {
                return Err(SecretError::MalformedCanonical);
            }
        }
        Ok(Self { values })
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.values.keys().map(String::as_str)
    }

    pub fn get(&self, name: &str) -> Option<&[u8]> {
        self.values.get(name).map(|value| value.as_slice())
    }

    pub fn remove(&mut self, name: &str) -> bool {
        self.values.remove(name).is_some()
    }

    pub fn insert(&mut self, name: String, value: Vec<u8>) -> Result<bool, SecretError> {
        validate_secret_name(&name)?;
        validate_secret_value(&value)?;
        if self
            .values
            .keys()
            .any(|existing| existing.eq_ignore_ascii_case(&name) && existing != &name)
        {
            return Err(SecretError::InvalidName(name));
        }
        Ok(self.values.insert(name, Zeroizing::new(value)).is_some())
    }

    fn pairs(&self) -> impl Iterator<Item = (&str, &[u8])> {
        self.values
            .iter()
            .map(|(name, value)| (name.as_str(), value.as_slice()))
    }
}

impl fmt::Debug for SecretMap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretMap")
            .field("names", &self.values.keys().collect::<Vec<_>>())
            .finish()
    }
}

pub fn validate_secret_name(name: &str) -> Result<(), SecretError> {
    if name.is_empty()
        || name.len() > 256
        || !name.bytes().enumerate().all(|(index, byte)| {
            if index == 0 {
                byte == b'_' || byte.is_ascii_alphabetic()
            } else {
                byte == b'_' || byte.is_ascii_alphanumeric()
            }
        })
    {
        return Err(SecretError::InvalidName(name.to_owned()));
    }
    Ok(())
}

pub fn validate_secret_value(value: &[u8]) -> Result<(), SecretError> {
    if value.len() > MAX_VALUE_BYTES || value.contains(&0) || std::str::from_utf8(value).is_err() {
        return Err(if value.len() > MAX_VALUE_BYTES {
            SecretError::ValueTooLarge
        } else {
            SecretError::InvalidValue
        });
    }
    Ok(())
}

pub fn parse_dotenv(input: &[u8]) -> Result<SecretMap, SecretError> {
    let text = std::str::from_utf8(input).map_err(|_| SecretError::InvalidValue)?;
    let mut pairs = Vec::new();
    for (line_number, raw_line) in text.lines().enumerate() {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            return Err(SecretError::MalformedDotenvLine(line_number + 1));
        };
        if name.is_empty()
            || name.trim() != name
            || value.starts_with('"')
            || value.starts_with('\'')
        {
            return Err(SecretError::MalformedDotenvLine(line_number + 1));
        }
        validate_secret_name(name)?;
        let _ = line_number;
        pairs.push((name.to_owned(), value.as_bytes().to_vec()));
    }
    SecretMap::from_pairs(pairs)
}

#[derive(Clone, Eq, PartialEq)]
pub struct EnvironmentGeneration {
    pub generation: u64,
    pub member_wrap: Vec<u8>,
    pub history_links: BTreeMap<u64, Vec<u8>>,
}

impl fmt::Debug for EnvironmentGeneration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentGeneration")
            .field("generation", &self.generation)
            .field("member_wrap_bytes", &self.member_wrap.len())
            .field(
                "history_generations",
                &self.history_links.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct Keyring {
    pub secrets_domain: [u8; 32],
    pub previous: Option<ArtifactId>,
    pub signer: MemberId,
    pub members: BTreeMap<MemberId, MemberIdentity>,
    pub environments: BTreeMap<String, EnvironmentGeneration>,
    signature: [u8; 64],
}

impl fmt::Debug for Keyring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Keyring")
            .field("secrets_domain", &MemberId(self.secrets_domain))
            .field("previous", &self.previous)
            .field("signer", &self.signer)
            .field("members", &self.members.keys().collect::<Vec<_>>())
            .field(
                "environments",
                &self.environments.keys().collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl Keyring {
    pub fn genesis(secrets_domain: [u8; 32], credential: &Credential) -> Result<Self, SecretError> {
        let member = credential.member_identity();
        let mut members = BTreeMap::new();
        members.insert(member.member_id(), member.clone());
        let mut keyring = Self {
            secrets_domain,
            previous: None,
            signer: member.member_id(),
            members,
            environments: BTreeMap::new(),
            signature: [0; 64],
        };
        keyring.sign(credential)?;
        Ok(keyring)
    }

    pub fn sign(&mut self, credential: &Credential) -> Result<(), SecretError> {
        let member = credential.member_identity();
        if self.signer != member.member_id() {
            return Err(SecretError::InvalidSignature);
        }
        let signature = credential
            .signing_key()
            .sign(&signature_input(&self.canonical_unsigned()));
        self.signature = signature.to_bytes();
        Ok(())
    }

    pub fn verify(&self) -> Result<(), SecretError> {
        self.verify_member(
            self.members
                .get(&self.signer)
                .ok_or(SecretError::InvalidSignature)?,
            &self.canonical_unsigned(),
        )
    }

    pub fn verify_against(&self, predecessor: Option<&Keyring>) -> Result<(), SecretError> {
        match (self.previous, predecessor) {
            (None, None) => self.verify(),
            (Some(previous), Some(predecessor)) => {
                if previous != predecessor.artifact()?.id() {
                    return Err(SecretError::InvalidPredecessor);
                }
                let member = predecessor
                    .members
                    .get(&self.signer)
                    .ok_or(SecretError::InvalidPredecessor)?;
                self.verify_member(member, &self.canonical_unsigned())
            }
            _ => Err(SecretError::InvalidPredecessor),
        }
    }

    fn verify_member(&self, member: &MemberIdentity, unsigned: &[u8]) -> Result<(), SecretError> {
        member.verify_id()?;
        let key = VerifyingKey::from_bytes(&member.signing_public)
            .map_err(|_| SecretError::InvalidPublicKey)?;
        key.verify_strict(
            &signature_input(unsigned),
            &Signature::from_bytes(&self.signature),
        )
        .map_err(|_| SecretError::InvalidSignature)
    }

    pub fn canonical_unsigned(&self) -> Vec<u8> {
        encode_keyring_unsigned(self)
    }

    pub fn artifact(&self) -> Result<Artifact, SecretError> {
        Ok(Artifact::new(ArtifactKind::Keyring, &encode_keyring(self))?)
    }

    pub fn from_artifact(artifact: Artifact) -> Result<Self, SecretError> {
        if artifact.kind() != ArtifactKind::Keyring {
            return Err(SecretError::WrongArtifactKind);
        }
        decode_keyring(artifact.payload())
    }

    pub fn environment(&self, name: &str) -> Result<&EnvironmentGeneration, SecretError> {
        self.environments
            .get(name)
            .ok_or_else(|| SecretError::MissingEnvironment(name.to_owned()))
    }

    pub fn unlock_environment(
        &self,
        name: &str,
        credential: &Credential,
    ) -> Result<EnvironmentIdentity, SecretError> {
        let generation = self.environment(name)?;
        let current = EnvironmentIdentity::unwrap_member_wrap(
            &generation.member_wrap,
            credential,
            self.secrets_domain,
            name,
            generation.generation,
        )?;
        let mut cursor = current.clone();
        while cursor.generation > 1 {
            let previous_generation = cursor.generation - 1;
            let link = generation
                .history_links
                .get(&previous_generation)
                .ok_or(SecretError::InvalidPredecessor)?;
            cursor = cursor.unwrap_history_link(link)?;
        }
        Ok(current)
    }
}

fn signature_input(unsigned: &[u8]) -> Vec<u8> {
    let mut input = Vec::with_capacity(KEYRING_SIGNATURE_LABEL.len() + 8 + unsigned.len());
    input.extend_from_slice(KEYRING_SIGNATURE_LABEL);
    input.extend_from_slice(&(unsigned.len() as u64).to_be_bytes());
    input.extend_from_slice(unsigned);
    input
}

#[derive(Clone)]
pub struct EnvironmentIdentity {
    pub secrets_domain: [u8; 32],
    pub environment: String,
    pub generation: u64,
    identity: x25519::Identity,
}

impl fmt::Debug for EnvironmentIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EnvironmentIdentity")
            .field("secrets_domain", &MemberId(self.secrets_domain))
            .field("environment", &self.environment)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

impl EnvironmentIdentity {
    pub fn generate(
        secrets_domain: [u8; 32],
        environment: String,
        generation: u64,
    ) -> Result<Self, SecretError> {
        validate_environment(&environment)?;
        Ok(Self {
            secrets_domain,
            environment,
            generation,
            identity: x25519::Identity::generate(),
        })
    }

    fn from_parts(
        secrets_domain: [u8; 32],
        environment: String,
        generation: u64,
        identity: x25519::Identity,
    ) -> Result<Self, SecretError> {
        validate_environment(&environment)?;
        Ok(Self {
            secrets_domain,
            environment,
            generation,
            identity,
        })
    }

    pub fn recipient(&self) -> String {
        self.identity.to_public().to_string().to_string()
    }

    pub fn wrap_for_members(
        &self,
        members: impl IntoIterator<Item = String>,
    ) -> Result<Vec<u8>, SecretError> {
        let mut plaintext = Vec::new();
        plaintext.extend_from_slice(ENV_IDENTITY_MAGIC);
        plaintext.extend_from_slice(&self.secrets_domain);
        put_string(&mut plaintext, &self.environment)?;
        plaintext.extend_from_slice(&self.generation.to_be_bytes());
        let identity = self.identity.to_string();
        put_string(&mut plaintext, identity.expose_secret())?;
        age_encrypt(members, &plaintext)
    }

    pub fn unwrap_member_wrap(
        ciphertext: &[u8],
        credential: &Credential,
        expected_domain: [u8; 32],
        expected_environment: &str,
        expected_generation: u64,
    ) -> Result<Self, SecretError> {
        let plaintext = age_decrypt(ciphertext, credential.age_identity())?;
        let mut reader = Reader::new(&plaintext);
        if reader.take(ENV_IDENTITY_MAGIC.len())? != ENV_IDENTITY_MAGIC {
            return Err(SecretError::WrongRouting);
        }
        let domain = reader.array_32()?;
        let environment = reader.string_u32()?;
        let generation = reader.u64()?;
        let identity_string = reader.string_u32()?;
        reader.finish()?;
        if domain != expected_domain
            || environment != expected_environment
            || generation != expected_generation
        {
            return Err(SecretError::WrongRouting);
        }
        let identity = identity_string
            .parse()
            .map_err(|_| SecretError::MalformedCanonical)?;
        Self::from_parts(domain, environment, generation, identity)
    }

    pub fn history_link(&self, previous: &EnvironmentIdentity) -> Result<Vec<u8>, SecretError> {
        if self.secrets_domain != previous.secrets_domain
            || self.environment != previous.environment
            || self.generation != previous.generation + 1
        {
            return Err(SecretError::WrongRouting);
        }
        let mut plaintext = Vec::new();
        plaintext.extend_from_slice(HISTORY_MAGIC);
        plaintext.extend_from_slice(&self.secrets_domain);
        put_string(&mut plaintext, &self.environment)?;
        plaintext.extend_from_slice(&self.generation.to_be_bytes());
        plaintext.extend_from_slice(&previous.generation.to_be_bytes());
        let identity = previous.identity.to_string();
        put_string(&mut plaintext, identity.expose_secret())?;
        age_encrypt(std::iter::once(self.recipient()), &plaintext)
    }

    pub fn unwrap_history_link(&self, ciphertext: &[u8]) -> Result<Self, SecretError> {
        let plaintext = age_decrypt(ciphertext, &self.identity)?;
        let mut reader = Reader::new(&plaintext);
        if reader.take(HISTORY_MAGIC.len())? != HISTORY_MAGIC {
            return Err(SecretError::WrongRouting);
        }
        let domain = reader.array_32()?;
        let environment = reader.string_u32()?;
        let generation = reader.u64()?;
        let previous_generation = reader.u64()?;
        let identity_string = reader.string_u32()?;
        reader.finish()?;
        if domain != self.secrets_domain
            || environment != self.environment
            || generation != self.generation
            || previous_generation + 1 != self.generation
        {
            return Err(SecretError::WrongRouting);
        }
        let identity = identity_string
            .parse()
            .map_err(|_| SecretError::MalformedCanonical)?;
        Self::from_parts(domain, environment, previous_generation, identity)
    }
}

pub struct SecretBundle {
    pub secrets_domain: [u8; 32],
    pub environment: String,
    pub generation: u64,
    ciphertext: Vec<u8>,
}

impl fmt::Debug for SecretBundle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretBundle")
            .field("secrets_domain", &MemberId(self.secrets_domain))
            .field("environment", &self.environment)
            .field("generation", &self.generation)
            .field("ciphertext_bytes", &self.ciphertext.len())
            .finish()
    }
}

impl SecretBundle {
    pub fn encrypt(
        identity: &EnvironmentIdentity,
        secrets: &SecretMap,
    ) -> Result<Self, SecretError> {
        let mut plaintext = Vec::new();
        plaintext.extend_from_slice(BUNDLE_MAGIC);
        plaintext.extend_from_slice(&identity.secrets_domain);
        put_string(&mut plaintext, &identity.environment)?;
        plaintext.extend_from_slice(&identity.generation.to_be_bytes());
        encode_secret_map(&mut plaintext, secrets)?;
        if plaintext.len() > MAX_BUNDLE_BYTES {
            return Err(SecretError::BundleTooLarge);
        }
        let ciphertext = age_encrypt(std::iter::once(identity.recipient()), &plaintext)?;
        Ok(Self {
            secrets_domain: identity.secrets_domain,
            environment: identity.environment.clone(),
            generation: identity.generation,
            ciphertext,
        })
    }

    pub fn decrypt(&self, identity: &EnvironmentIdentity) -> Result<SecretMap, SecretError> {
        if self.secrets_domain != identity.secrets_domain
            || self.environment != identity.environment
            || self.generation != identity.generation
        {
            return Err(SecretError::WrongRouting);
        }
        let plaintext = age_decrypt(&self.ciphertext, &identity.identity)?;
        let mut reader = Reader::new(&plaintext);
        if reader.take(BUNDLE_MAGIC.len())? != BUNDLE_MAGIC {
            return Err(SecretError::WrongRouting);
        }
        let domain = reader.array_32()?;
        let environment = reader.string_u32()?;
        let generation = reader.u64()?;
        let secrets = decode_secret_map(&mut reader)?;
        reader.finish()?;
        if domain != self.secrets_domain
            || environment != self.environment
            || generation != self.generation
        {
            return Err(SecretError::WrongRouting);
        }
        Ok(secrets)
    }

    pub fn artifact(&self) -> Result<Artifact, SecretError> {
        let mut payload = Vec::new();
        payload.extend_from_slice(&self.secrets_domain);
        put_string(&mut payload, &self.environment)?;
        payload.extend_from_slice(&self.generation.to_be_bytes());
        put_bytes_u64(&mut payload, &self.ciphertext)?;
        Ok(Artifact::new(ArtifactKind::SecretBundle, &payload)?)
    }

    pub fn from_artifact(artifact: Artifact) -> Result<Self, SecretError> {
        if artifact.kind() != ArtifactKind::SecretBundle {
            return Err(SecretError::WrongArtifactKind);
        }
        let mut reader = Reader::new(artifact.payload());
        let secrets_domain = reader.array_32()?;
        let environment = reader.string_u32()?;
        let generation = reader.u64()?;
        let ciphertext = reader.bytes_u64()?.to_vec();
        reader.finish()?;
        validate_environment(&environment)?;
        if ciphertext.is_empty() {
            return Err(SecretError::MalformedCanonical);
        }
        Ok(Self {
            secrets_domain,
            environment,
            generation,
            ciphertext,
        })
    }

    pub fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

pub fn create_empty_environment(
    store: &RepoStore,
    secrets_domain: [u8; 32],
    name: &str,
    credential: &Credential,
) -> Result<(ArtifactId, ArtifactId), SecretError> {
    let mut keyring = if let Some(id) = store.get_ref("secrets/keyring")? {
        Keyring::from_artifact(store.get(id)?.ok_or(SecretError::InvalidPredecessor)?)?
    } else {
        Keyring::genesis(secrets_domain, credential)?
    };
    if keyring.secrets_domain != secrets_domain {
        return Err(SecretError::WrongRouting);
    }
    if keyring.environments.contains_key(name) {
        return Err(SecretError::InvalidEnvironment);
    }
    let current = EnvironmentIdentity::generate(secrets_domain, name.to_owned(), 1)?;
    let member_wrap = current.wrap_for_members(
        keyring
            .members
            .values()
            .map(|member| member.age_recipient.clone()),
    )?;
    keyring.environments.insert(
        name.to_owned(),
        EnvironmentGeneration {
            generation: 1,
            member_wrap,
            history_links: BTreeMap::new(),
        },
    );
    keyring.previous = store.get_ref("secrets/keyring")?;
    keyring.sign(credential)?;
    let bundle = SecretBundle::encrypt(&current, &SecretMap::empty())?;
    let keyring_artifact = keyring.artifact()?;
    let bundle_artifact = bundle.artifact()?;
    let keyring_id = store.insert(&keyring_artifact)?;
    let bundle_id = store.insert(&bundle_artifact)?;
    let expected_keyring = keyring.previous;
    store.cas_refs(&[
        crate::RefUpdate {
            name: "secrets/keyring".into(),
            expected: expected_keyring,
            new: Some(keyring_id),
        },
        crate::RefUpdate {
            name: format!("secrets/{name}"),
            expected: None,
            new: Some(bundle_id),
        },
    ])?;
    Ok((keyring_id, bundle_id))
}

fn encode_keyring(keyring: &Keyring) -> Vec<u8> {
    let mut output = encode_keyring_unsigned(keyring);
    output.extend_from_slice(&keyring.signature);
    output
}

fn encode_keyring_unsigned(keyring: &Keyring) -> Vec<u8> {
    let mut output = Vec::new();
    output.extend_from_slice(KEYRING_MAGIC);
    output.extend_from_slice(&keyring.secrets_domain);
    match keyring.previous {
        Some(id) => {
            output.push(1);
            output.extend_from_slice(id.as_slice());
        }
        None => output.push(0),
    }
    output.extend_from_slice(&keyring.signer.as_bytes());
    put_u32(&mut output, keyring.members.len()).expect("member count fits");
    for (id, member) in &keyring.members {
        output.extend_from_slice(&id.as_bytes());
        put_string(&mut output, &member.age_recipient).expect("recipient fits");
        output.extend_from_slice(&member.signing_public);
    }
    put_u32(&mut output, keyring.environments.len()).expect("environment count fits");
    for (name, environment) in &keyring.environments {
        put_string(&mut output, name).expect("environment fits");
        output.extend_from_slice(&environment.generation.to_be_bytes());
        put_bytes_u64(&mut output, &environment.member_wrap).expect("wrap fits");
        put_u32(&mut output, environment.history_links.len()).expect("history count fits");
        for (generation, link) in &environment.history_links {
            output.extend_from_slice(&generation.to_be_bytes());
            put_bytes_u64(&mut output, link).expect("link fits");
        }
    }
    output
}

fn decode_keyring(bytes: &[u8]) -> Result<Keyring, SecretError> {
    if bytes.len() < 64 {
        return Err(SecretError::MalformedCanonical);
    }
    let split = bytes.len() - 64;
    let unsigned = &bytes[..split];
    let signature: [u8; 64] = bytes[split..]
        .try_into()
        .map_err(|_| SecretError::MalformedCanonical)?;
    let mut reader = Reader::new(unsigned);
    if reader.take(KEYRING_MAGIC.len())? != KEYRING_MAGIC {
        return Err(SecretError::MalformedCanonical);
    }
    let secrets_domain = reader.array_32()?;
    let previous = match reader.u8()? {
        0 => None,
        1 => Some(ArtifactId::from_bytes(reader.array_32()?)),
        _ => return Err(SecretError::MalformedCanonical),
    };
    let signer = MemberId::from_bytes(reader.array_32()?);
    let member_count = reader.count()?;
    let mut members = BTreeMap::new();
    let mut previous_member = None;
    for _ in 0..member_count {
        let id = MemberId::from_bytes(reader.array_32()?);
        if previous_member.is_some_and(|previous| previous >= id) {
            return Err(SecretError::MalformedCanonical);
        }
        previous_member = Some(id);
        let age_recipient = reader.string_u32()?;
        let signing_public = reader.array_32()?;
        let member = MemberIdentity::new(age_recipient, signing_public)?;
        if member.member_id() != id || members.insert(id, member).is_some() {
            return Err(SecretError::InvalidMember);
        }
    }
    let environment_count = reader.count()?;
    let mut environments = BTreeMap::new();
    let mut previous_environment = None;
    for _ in 0..environment_count {
        let name = reader.string_u32()?;
        if previous_environment
            .as_deref()
            .is_some_and(|previous| previous >= name.as_str())
        {
            return Err(SecretError::MalformedCanonical);
        }
        previous_environment = Some(name.clone());
        validate_environment(&name)?;
        let generation = reader.u64()?;
        let member_wrap = reader.bytes_u64()?.to_vec();
        let history_count = reader.count()?;
        let mut history_links = BTreeMap::new();
        let mut previous_generation = None;
        for _ in 0..history_count {
            let generation_key = reader.u64()?;
            if previous_generation.is_some_and(|previous| previous >= generation_key) {
                return Err(SecretError::MalformedCanonical);
            }
            previous_generation = Some(generation_key);
            let link = reader.bytes_u64()?.to_vec();
            if history_links.insert(generation_key, link).is_some() {
                return Err(SecretError::MalformedCanonical);
            }
        }
        if environments
            .insert(
                name,
                EnvironmentGeneration {
                    generation,
                    member_wrap,
                    history_links,
                },
            )
            .is_some()
        {
            return Err(SecretError::MalformedCanonical);
        }
    }
    reader.finish()?;
    let keyring = Keyring {
        secrets_domain,
        previous,
        signer,
        members,
        environments,
        signature,
    };
    if keyring.canonical_unsigned() != unsigned {
        return Err(SecretError::MalformedCanonical);
    }
    for member in keyring.members.values() {
        member.verify_id()?;
    }
    Ok(keyring)
}

fn encode_secret_map(output: &mut Vec<u8>, secrets: &SecretMap) -> Result<(), SecretError> {
    put_u32(output, secrets.values.len()).map_err(|_| SecretError::BundleTooLarge)?;
    for (name, value) in secrets.pairs() {
        put_string(output, name).map_err(|_| SecretError::BundleTooLarge)?;
        put_bytes_u64(output, value).map_err(|_| SecretError::BundleTooLarge)?;
    }
    Ok(())
}

fn decode_secret_map(reader: &mut Reader<'_>) -> Result<SecretMap, SecretError> {
    let count = reader.count()?;
    let mut pairs = Vec::with_capacity(count);
    let mut previous_name = None;
    for _ in 0..count {
        let name = reader.string_u32()?;
        if previous_name
            .as_deref()
            .is_some_and(|previous| previous >= name.as_str())
        {
            return Err(SecretError::MalformedCanonical);
        }
        previous_name = Some(name.clone());
        let value = reader.bytes_u64()?.to_vec();
        pairs.push((name, value));
    }
    SecretMap::from_pairs(pairs)
}

fn validate_environment(name: &str) -> Result<(), SecretError> {
    if name.is_empty() || name.contains('\0') || name.contains('/') || name == "." || name == ".." {
        Err(SecretError::InvalidEnvironment)
    } else {
        Ok(())
    }
}

fn age_encrypt<I>(recipients: I, plaintext: &[u8]) -> Result<Vec<u8>, SecretError>
where
    I: IntoIterator<Item = String>,
{
    let parsed = recipients
        .into_iter()
        .map(|recipient| {
            recipient
                .parse::<x25519::Recipient>()
                .map_err(|_| SecretError::AgeEncrypt)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let refs = parsed
        .iter()
        .map(|recipient| recipient as &dyn AgeRecipient);
    let encryptor = Encryptor::with_recipients(refs).map_err(|_| SecretError::AgeEncrypt)?;
    let mut ciphertext = Vec::new();
    let mut writer = encryptor
        .wrap_output(&mut ciphertext)
        .map_err(|_| SecretError::AgeEncrypt)?;
    writer
        .write_all(plaintext)
        .map_err(|_| SecretError::AgeEncrypt)?;
    writer.finish().map_err(|_| SecretError::AgeEncrypt)?;
    Ok(ciphertext)
}

fn age_decrypt(ciphertext: &[u8], identity: &x25519::Identity) -> Result<Vec<u8>, SecretError> {
    let decryptor =
        Decryptor::new_buffered(Cursor::new(ciphertext)).map_err(|_| SecretError::AgeDecrypt)?;
    let mut reader = decryptor
        .decrypt(std::iter::once(identity as &dyn AgeIdentity))
        .map_err(|_| SecretError::AgeDecrypt)?;
    let mut plaintext = Vec::new();
    reader
        .read_to_end(&mut plaintext)
        .map_err(|_| SecretError::AgeDecrypt)?;
    Ok(plaintext)
}

fn put_u32(output: &mut Vec<u8>, value: usize) -> Result<(), SecretError> {
    output.extend_from_slice(
        &u32::try_from(value)
            .map_err(|_| SecretError::MalformedCanonical)?
            .to_be_bytes(),
    );
    Ok(())
}

fn put_string(output: &mut Vec<u8>, value: &str) -> Result<(), SecretError> {
    put_u32(output, value.len())?;
    output.extend_from_slice(value.as_bytes());
    Ok(())
}
fn put_bytes_u64(output: &mut Vec<u8>, value: &[u8]) -> Result<(), SecretError> {
    output.extend_from_slice(
        &u64::try_from(value.len())
            .map_err(|_| SecretError::MalformedCanonical)?
            .to_be_bytes(),
    );
    output.extend_from_slice(value);
    Ok(())
}

struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Reader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }
    fn take(&mut self, length: usize) -> Result<&'a [u8], SecretError> {
        let end = self
            .position
            .checked_add(length)
            .ok_or(SecretError::MalformedCanonical)?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or(SecretError::MalformedCanonical)?;
        self.position = end;
        Ok(value)
    }
    fn u8(&mut self) -> Result<u8, SecretError> {
        Ok(self.take(1)?[0])
    }
    fn u64(&mut self) -> Result<u64, SecretError> {
        Ok(u64::from_be_bytes(
            self.take(8)?
                .try_into()
                .map_err(|_| SecretError::MalformedCanonical)?,
        ))
    }
    fn count(&mut self) -> Result<usize, SecretError> {
        usize::try_from(u32::from_be_bytes(
            self.take(4)?
                .try_into()
                .map_err(|_| SecretError::MalformedCanonical)?,
        ))
        .map_err(|_| SecretError::MalformedCanonical)
    }
    fn bytes_u32(&mut self) -> Result<&'a [u8], SecretError> {
        let length = self.count()?;
        self.take(length)
    }
    fn bytes_u64(&mut self) -> Result<&'a [u8], SecretError> {
        let length = usize::try_from(self.u64()?).map_err(|_| SecretError::MalformedCanonical)?;
        self.take(length)
    }
    fn string_u32(&mut self) -> Result<String, SecretError> {
        String::from_utf8(self.bytes_u32()?.to_vec()).map_err(|_| SecretError::MalformedCanonical)
    }
    fn array_32(&mut self) -> Result<[u8; 32], SecretError> {
        self.take(32)?
            .try_into()
            .map_err(|_| SecretError::MalformedCanonical)
    }
    fn finish(&self) -> Result<(), SecretError> {
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(SecretError::MalformedCanonical)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_file() -> PathBuf {
        std::env::temp_dir().join(format!(
            "graft-credential-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ))
    }

    #[test]
    fn credential_round_trips_and_member_fingerprint_covers_both_keys() {
        let path = temp_file();
        let credential = Credential::generate();
        credential.save_atomic(&path).expect("save");
        let loaded = Credential::load(&path).expect("load");
        assert_eq!(
            loaded.member_identity().member_id(),
            credential.member_identity().member_id()
        );
        assert_ne!(credential.member_identity().age_recipient(), "");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn age_bundle_and_keyring_signature_round_trip() {
        let credential = Credential::generate();
        let domain = [7; 32];
        let identity = EnvironmentIdentity::generate(domain, "dev".into(), 1).expect("identity");
        let mut map = SecretMap::empty();
        map.insert("TOKEN".into(), b"marker".to_vec())
            .expect("secret");
        let bundle = SecretBundle::encrypt(&identity, &map).expect("bundle");
        assert_eq!(
            bundle.decrypt(&identity).expect("decrypt").get("TOKEN"),
            Some(&b"marker"[..])
        );
        let mut keyring = Keyring::genesis(domain, &credential).expect("genesis");
        keyring.environments.insert(
            "dev".into(),
            EnvironmentGeneration {
                generation: 1,
                member_wrap: identity
                    .wrap_for_members(std::iter::once(
                        credential.member_identity().age_recipient().to_owned(),
                    ))
                    .expect("wrap"),
                history_links: BTreeMap::new(),
            },
        );
        keyring.sign(&credential).expect("resign");
        let artifact = keyring.artifact().expect("artifact");
        let parsed = Keyring::from_artifact(artifact).expect("keyring");
        parsed.verify().expect("signature");
    }

    #[test]
    fn input_contract_rejects_argv_style_values_and_non_literal_dotenv() {
        assert!(validate_secret_name("A-B").is_err());
        assert!(SecretMap::from_pairs([
            (String::from("A"), b"one".to_vec()),
            (String::from("a"), b"two".to_vec())
        ])
        .is_err());
        assert!(parse_dotenv(b"export A=one\n").is_err());
        assert_eq!(
            parse_dotenv(b"# comment\nA=one\n")
                .expect("dotenv")
                .get("A"),
            Some(&b"one"[..])
        );
    }
}
