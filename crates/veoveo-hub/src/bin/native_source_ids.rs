//! Native source identity syntax shared only by Hub-package executable adapters
//! and their client DTOs. Generic Hub/Core/SDK remain native-name-independent.
#[veoveo_types::id(text(veoveo_types::identifier_syntax::PathIdProfile))]
pub struct DeploymentId(String);
#[veoveo_types::id(text(RepositoryIds))]
pub struct RepositoryId(String);
#[doc(hidden)]
pub struct RepositoryIds;
impl veoveo_types::IdProfile for RepositoryIds {
    type Error = veoveo_types::IdentifierError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| {
            if value.len() == 32
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                Ok(())
            } else {
                Err(veoveo_types::IdentifierError::new(
                    value,
                    "expected a canonical native repository ID",
                ))
            }
        });
}

#[veoveo_types::id(text(RepositoryIds))]
pub struct BranchId(String);
#[veoveo_types::id(text(NativeRevisions))]
pub struct NativeRevision(String);
#[doc(hidden)]
pub struct NativeRevisions;
impl veoveo_types::IdProfile for NativeRevisions {
    type Error = veoveo_types::IdentifierError;
    const PROFILE: veoveo_types::IdProfileSpec<Self::Error> =
        veoveo_types::IdProfileSpec::text(|value, _| {
            if value.len() == 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                Ok(())
            } else {
                Err(veoveo_types::IdentifierError::new(
                    value,
                    "expected native immutable revision ID",
                ))
            }
        });
}
