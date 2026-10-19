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
