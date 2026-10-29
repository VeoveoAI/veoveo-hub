#[path = "../../../../crates/veoveo-hub/src/delegation/authority.rs"]
pub mod authority;

#[cfg(test)]
mod tests {
    use super::authority::{self, Permissions};

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Set(u8);

    impl Permissions<u8> for Set {
        fn contains(&self, permission: &u8) -> bool {
            self.0 & (1 << permission) != 0
        }
        fn is_empty(&self) -> bool {
            self.0 == 0
        }
        fn narrow(self, carried: &Self) -> Self {
            Self(self.0 & carried.0)
        }
    }
    impl authority::Uses<Set, u8> for Set {
        fn first_undeclared(&self, requested: &Set) -> Option<u8> {
            (0..8).find(|permission| requested.contains(permission) && !self.contains(permission))
        }
    }
    impl authority::Held<Set> for Set {
        fn held(&self, requested: &Set) -> Set {
            requested.narrow(self)
        }
    }
    impl authority::ResourceKey<u8> for u8 {
        fn matches(&self, requested: &u8) -> bool {
            self == requested
        }
    }

    #[test]
    fn exchange_keeps_current_authority_and_every_original_context_field() {
        let declared = authority::declare(Set(0b111), &Set(0b111)).unwrap();
        let addressed =
            authority::address(declared, authority::Destination::Resource(19), Some(&19)).unwrap();
        let provided = authority::provide(addressed, &[] as &[u8]).unwrap();
        let granted = authority::grant(
            provided,
            &Set(0b110),
            authority::Context {
                profile: 23,
                caller: 29,
                actor: 31,
                task: Some(37),
                deadline: 41,
            },
        );
        assert_eq!(granted.permissions, Set(0b110));
        assert_eq!(
            (
                granted.resource,
                granted.context.profile,
                granted.context.caller,
                granted.context.actor,
                granted.context.task,
                granted.context.deadline
            ),
            (19, 23, 29, 31, Some(37), 41)
        );
    }

    #[test]
    fn undeclared_use_is_refused_before_destination_and_provider_decisions() {
        assert!(matches!(
            authority::declare(Set(0b111), &Set(0b101)),
            Err(1)
        ));
        let declared = authority::declare(Set(0), &Set(0)).unwrap();
        assert!(matches!(
            authority::address(
                declared,
                authority::Destination::<u8>::MissingProfile,
                None::<&u8>
            ),
            Err(authority::TargetRefusal::MissingProfile)
        ));
    }

    #[test]
    fn wrong_resource_and_missing_provider_do_not_issue_a_grant() {
        let declared = authority::declare(Set(0b1), &Set(0b1)).unwrap();
        assert!(matches!(
            authority::address(declared, authority::Destination::Resource(19), Some(&20)),
            Err(authority::TargetRefusal::OtherResource(19))
        ));
        let declared = authority::declare(Set(0b1), &Set(0b1)).unwrap();
        let addressed =
            authority::address(declared, authority::Destination::Resource(19), Some(&19)).unwrap();
        assert!(matches!(
            authority::provide(addressed, &[0]),
            Err(authority::TargetRefusal::MissingProviders)
        ));
    }

    #[test]
    fn carried_authority_narrows_permissions_without_replacing_decision_metadata() {
        let output = authority::narrow(
            authority::Decision {
                allowed: true,
                reason: 7,
                granted: Set(0b111),
                metadata: (11, 13, 17),
            },
            true,
            Some((&Set(0b010), authority::Route::Permission(&1))),
            19,
            23,
        );
        assert!(output.allowed);
        assert_eq!(
            (output.reason, output.granted, output.metadata),
            (7, Set(0b010), (11, 13, 17))
        );
    }

    #[test]
    fn authenticated_empty_is_valid_but_undeclared_and_empty_server_are_refused() {
        let decision = || authority::Decision {
            allowed: true,
            reason: 7,
            granted: Set(0),
            metadata: 11,
        };
        let output = authority::narrow(
            decision(),
            true,
            Some((&Set(0), authority::Route::<u8>::Authenticated)),
            19,
            23,
        );
        assert!(output.allowed);
        assert_eq!(output.granted, Set(0));
        for route in [authority::Route::<u8>::Undeclared, authority::Route::Server] {
            let output = authority::narrow(decision(), true, Some((&Set(0), route)), 19, 23);
            assert!(!output.allowed);
            assert_eq!((output.reason, output.metadata), (23, 11));
        }
    }

    #[test]
    fn prior_denial_and_managed_denial_keep_the_original_permission_payload() {
        let output = authority::narrow(
            authority::Decision {
                allowed: false,
                reason: 7,
                granted: Set(0b111),
                metadata: 11,
            },
            true,
            Some((&Set(0), authority::Route::<u8>::Undeclared)),
            19,
            23,
        );
        assert!(!output.allowed);
        assert_eq!(
            (output.reason, output.granted, output.metadata),
            (7, Set(0b111), 11)
        );
        let output = authority::narrow(
            authority::Decision {
                allowed: true,
                reason: 7,
                granted: Set(0b111),
                metadata: 11,
            },
            false,
            Some((&Set(0), authority::Route::<u8>::Authenticated)),
            19,
            23,
        );
        assert!(!output.allowed);
        assert_eq!(
            (output.reason, output.granted, output.metadata),
            (19, Set(0b111), 11)
        );
    }
}
