//! Pure delegated-authority decisions; native finite-set operations are supplied by
//! the production adapter. Their exact semantics are explicit proof premises.

pub trait Permissions<P> {
    fn contains(&self, permission: &P) -> bool;
    fn is_empty(&self) -> bool;
    fn narrow(self, carried: &Self) -> Self;
}

pub trait Uses<S, P> {
    fn first_undeclared(&self, requested: &S) -> Option<P>;
}

pub struct Declared<S> {
    requested: S,
}

pub fn declare<S, P, U: Uses<S, P>>(requested: S, uses: &U) -> Result<Declared<S>, P> {
    match uses.first_undeclared(&requested) {
        Some(permission) => Err(permission),
        None => Ok(Declared { requested }),
    }
}

pub trait Held<S> {
    fn held(&self, requested: &S) -> S;
}

pub trait ResourceKey<K> {
    fn matches(&self, requested: &K) -> bool;
}

pub enum Destination<R> {
    MissingProfile,
    InvalidResource,
    Resource(R),
}

#[derive(Debug, PartialEq, Eq)]
pub enum TargetRefusal<R> {
    MissingProfile,
    InvalidResource,
    OtherResource(R),
    MissingProviders,
}

pub struct Addressed<S, R> {
    pub(super) requested: S,
    resource: R,
}

pub fn address<S, K, R: ResourceKey<K>>(
    declared: Declared<S>,
    destination: Destination<R>,
    requested_resource: Option<&K>,
) -> Result<Addressed<S, R>, TargetRefusal<R>> {
    let resource = match destination {
        Destination::MissingProfile => return Err(TargetRefusal::MissingProfile),
        Destination::InvalidResource => return Err(TargetRefusal::InvalidResource),
        Destination::Resource(resource) => resource,
    };
    match requested_resource {
        Some(requested) => {
            if !resource.matches(requested) {
                return Err(TargetRefusal::OtherResource(resource));
            }
        }
        None => return Err(TargetRefusal::OtherResource(resource)),
    }
    Ok(Addressed {
        requested: declared.requested,
        resource,
    })
}

pub struct Provided<S, R> {
    requested: S,
    resource: R,
}

pub fn provide<S, R, P>(
    addressed: Addressed<S, R>,
    missing: &[P],
) -> Result<Provided<S, R>, TargetRefusal<R>> {
    if !missing.is_empty() {
        return Err(TargetRefusal::MissingProviders);
    }
    Ok(Provided {
        requested: addressed.requested,
        resource: addressed.resource,
    })
}

pub struct Context<F, C, A, T, D> {
    pub profile: F,
    pub caller: C,
    pub actor: A,
    pub task: T,
    pub deadline: D,
}

pub struct Grant<S, R, F, C, A, T, D> {
    pub permissions: S,
    pub resource: R,
    pub context: Context<F, C, A, T, D>,
}

pub fn grant<S, H: Held<S>, R, F, C, A, T, D>(
    provided: Provided<S, R>,
    held: &H,
    context: Context<F, C, A, T, D>,
) -> Grant<S, R, F, C, A, T, D> {
    Grant {
        permissions: held.held(&provided.requested),
        resource: provided.resource,
        context,
    }
}

pub enum Route<'a, P> {
    Authenticated,
    Permission(&'a P),
    Server,
    Undeclared,
}

pub struct Decision<S, R, M> {
    pub allowed: bool,
    pub reason: R,
    pub granted: S,
    pub metadata: M,
}

pub fn narrow<S: Permissions<P>, P, R, M>(
    mut decision: Decision<S, R, M>,
    managed: bool,
    delegation: Option<(&S, Route<'_, P>)>,
    policy_deny: R,
    missing_permission: R,
) -> Decision<S, R, M> {
    if !managed {
        decision.allowed = false;
        decision.reason = policy_deny;
    }
    if !decision.allowed {
        return decision;
    }
    if let Some((carried, route)) = delegation {
        decision.granted = decision.granted.narrow(carried);
        let allowed = match route {
            Route::Authenticated => true,
            Route::Permission(permission) => carried.contains(permission),
            Route::Server => !decision.granted.is_empty(),
            Route::Undeclared => false,
        };
        if !allowed {
            decision.allowed = false;
            decision.reason = missing_permission;
        }
    }
    decision
}
