use sha2::{Digest, Sha256};
use veoveo_types::{
    IdFailure, IdMetadata, IdProfile, IdProfileSpec, IdentifierError, UuidGrammar, UuidSpelling,
    UuidVariant,
};

/// Opaque public route identity. Possession never grants access to a Computer.
#[veoveo_types::id(uuid(ComputerIds), fresh)]
pub struct ComputerId(String);

#[doc(hidden)]
pub struct ComputerIds;
impl IdProfile for ComputerIds {
    type Error = IdentifierError;
    const PROFILE: IdProfileSpec<Self::Error> = IdProfileSpec::generated_uuid(
        UuidGrammar {
            versions: &[7],
            variant: UuidVariant::Rfc4122,
            spelling: UuidSpelling::CanonicalLowerHyphenated,
        },
        invalid_id,
    );
}
fn invalid_id(value: &str, _: IdMetadata, _: IdFailure) -> IdentifierError {
    IdentifierError::new(value, "must be a canonical RFC UUIDv7")
}

/// Native names encode IDs in these lowercase base32 characters.
const NATIVE_ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
const NATIVE_NAME_LENGTH: usize = 19;

impl ComputerId {
    /// Native OpenShell DNS-routable workspace/sandbox names are limited to 19
    /// characters. Keep the full ID in Hub ownership and the cloned base path. The
    /// name also identifies the Computer's own Lore client; it is never UUID-shaped.
    pub fn native_name(&self) -> String {
        let digest = Sha256::digest(self.as_uuid().as_bytes());
        let mut name = String::with_capacity(NATIVE_NAME_LENGTH);
        name.push('c');
        // Encode the first 90 digest bits as 18 lowercase base32 characters.
        for bit in (0..90).step_by(5) {
            let byte = bit / 8;
            let window = (u16::from(digest[byte]) << 8) | u16::from(digest[byte + 1]);
            let index = usize::from((window >> (11 - bit % 8)) & 31);
            name.push(char::from(NATIVE_ALPHABET[index]));
        }
        name
    }
}

/// Whether `value` is shaped like a Computer's native name: `c` and 18 lowercase
/// base32 characters.
pub fn is_native_name(value: &str) -> bool {
    value.len() == NATIVE_NAME_LENGTH
        && value.starts_with('c')
        && value.bytes().all(|b| NATIVE_ALPHABET.contains(&b))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_names_are_routable_and_never_uuid_shaped() {
        let id = ComputerId::new();
        let name = id.native_name();
        assert_eq!(name, id.native_name());
        assert!(is_native_name(&name), "{name}");
        assert!(uuid::Uuid::parse_str(&name).is_err());
        assert!(!is_native_name(id.as_ref()));
        assert!(!is_native_name(&uuid::Uuid::now_v7().simple().to_string()));
        assert!(!is_native_name(&name.to_uppercase()));
        assert!(!is_native_name(&name[1..]));
    }
}
