//! Pure instance resolution, shared verbatim with the Rust-to-Lean extraction crate.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matches<D> {
    Missing,
    Unique(D),
    Ambiguous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Failure<B> {
    SelectedMissing(B),
    BaseMissing(B),
    NoBase,
    Ambiguous(B),
}

/// Classify the entire slice, distinguishing a unique deployment from duplicates.
pub fn lookup<B: PartialEq + Copy, D: Copy>(served: &[(B, D)], branch: B) -> Matches<D> {
    let mut found = Matches::Missing;
    let mut index = 0;
    while index < served.len() {
        let (candidate, deployment) = served[index];
        if candidate == branch {
            match found {
                Matches::Missing => found = Matches::Unique(deployment),
                Matches::Unique(_) | Matches::Ambiguous => return Matches::Ambiguous,
            }
        }
        index += 1;
    }
    found
}

/// An explicit selection owns the result, including failure; base applies only
/// when there is no explicit selection.
pub fn resolve<B: PartialEq + Copy, D: Copy>(
    served: &[(B, D)],
    selected: Option<B>,
    base: Option<B>,
) -> Result<D, Failure<B>> {
    match selected {
        Some(branch) => match lookup(served, branch) {
            Matches::Missing => Err(Failure::SelectedMissing(branch)),
            Matches::Unique(deployment) => Ok(deployment),
            Matches::Ambiguous => Err(Failure::Ambiguous(branch)),
        },
        None => match base {
            Some(branch) => match lookup(served, branch) {
                Matches::Missing => Err(Failure::BaseMissing(branch)),
                Matches::Unique(deployment) => Ok(deployment),
                Matches::Ambiguous => Err(Failure::Ambiguous(branch)),
            },
            None => Err(Failure::NoBase),
        },
    }
}
