#[path = "../../../../crates/veoveo-hub/src/state/task_routes/decision.rs"]
pub mod decision;

// The SDK is a separate, exact sibling checkout, not a copied verifier algorithm.
#[path = "../../../../../veoveo-hub-sdk/crates/veoveo-hub-sdk/src/internal_auth/recipient.rs"]
pub mod recipient;

#[cfg(test)]
mod tests {
    use super::{decision, recipient};

    #[test]
    fn retained_capture_and_current_caller_reach_the_exact_recipient() {
        let request = decision::admit_owner(
            7_u8,
            decision::Ownership {
                identity: 11_u8,
                labels: &[2_u8, 2],
            },
            decision::Task {
                server: 3_u8,
                deployment: 4_u8,
                canonical: 8_u8,
                source: 5_u8,
                capture: 6_u8,
            },
            decision::Caller {
                binding: 7_u8,
                ownership: decision::Ownership {
                    identity: 11_u8,
                    labels: &[9_u8, 2],
                },
                delegated: false,
                capture: Some(99_u8),
                payload: 13_u8,
            },
        )
        .unwrap();
        let captured = decision::capture(request, 6, Some(4)).unwrap();
        assert_eq!(captured.task.capture, 6);
        assert_eq!(captured.caller, 13);
        let emitted = recipient::Recipient {
            server: captured.task.server,
            deployment: captured.task.deployment,
            resource: 17_u8,
        };
        assert_eq!(recipient::bind(emitted, emitted), Ok(emitted));
    }

    #[test]
    fn retry_preserves_original_record_and_refuses_expiry_boundary() {
        let retained = decision::Retained {
            key: 7_u8,
            capture: 6_u8,
            expires: 20_i64,
            record: 9_u8,
        };
        assert_eq!(decision::reuse(retained, 7, 19), Ok(retained));
        assert_eq!(
            decision::reuse(retained, 7, 20),
            Err(decision::Refusal::Retry)
        );
        assert_eq!(
            decision::reuse(retained, 8, 19),
            Err(decision::Refusal::Retry)
        );
    }

    #[test]
    fn recipient_refuses_other_deployment_in_the_same_namespace_and_resource() {
        let presented = recipient::Recipient {
            server: 3_u8,
            deployment: 4_u8,
            resource: 17_u8,
        };
        assert_eq!(
            recipient::bind(
                recipient::Recipient {
                    deployment: 8,
                    ..presented
                },
                presented,
            ),
            Err(recipient::Mismatch::Deployment),
        );
    }

    #[test]
    fn wrong_selected_capture_deployment_is_refused() {
        let request = decision::TaskRequest {
            binding: 7_u8,
            task: decision::Task {
                server: 3_u8,
                deployment: 4_u8,
                canonical: 8_u8,
                source: 5_u8,
                capture: 6_u8,
            },
            caller: 13_u8,
        };
        assert_eq!(
            decision::capture(request, 6, Some(9)),
            Err(decision::Refusal::Capture),
        );
    }

    #[test]
    fn missing_retained_label_is_refused_for_an_otherwise_matching_owner() {
        let admitted = decision::admit_owner(
            7_u8,
            decision::Ownership {
                identity: 11_u8,
                labels: &[2_u8],
            },
            decision::Task {
                server: 3_u8,
                deployment: 4_u8,
                canonical: 8_u8,
                source: 5_u8,
                capture: 6_u8,
            },
            decision::Caller {
                binding: 7_u8,
                ownership: decision::Ownership {
                    identity: 11_u8,
                    labels: &[9_u8],
                },
                delegated: false,
                capture: Some(99_u8),
                payload: 13_u8,
            },
        );
        assert_eq!(admitted, Err(decision::Refusal::Ownership));
    }
}
