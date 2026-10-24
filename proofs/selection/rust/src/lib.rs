#[path = "../../../../crates/veoveo-hub/src/execution/selection/kernel.rs"]
pub mod kernel;

#[cfg(test)]
mod tests {
    use super::kernel;

    #[test]
    fn missing_explicit_selection_does_not_serve_the_available_base() {
        assert_eq!(
            kernel::resolve(&[(1_u8, 42_u64)], Some(2), Some(1)),
            Err(kernel::Failure::SelectedMissing(2))
        );
        assert_eq!(kernel::resolve(&[(1_u8, 42_u64)], None, Some(1)), Ok(42));
    }

    #[test]
    fn duplicate_entries_of_the_same_deployment_are_ambiguous() {
        let served = [(1_u8, 42_u64), (1, 42)];
        assert_eq!(kernel::lookup(&served, 1), kernel::Matches::Ambiguous);
        assert_eq!(
            kernel::resolve(&served, Some(1), Some(2)),
            Err(kernel::Failure::Ambiguous(1))
        );
    }
}
