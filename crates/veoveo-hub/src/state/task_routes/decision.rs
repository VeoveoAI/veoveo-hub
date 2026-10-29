//! Production Task decisions shared with the manual extraction package.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Binding<T, O, W, P> {
    pub tenant: T,
    pub owner: O,
    pub work_context: W,
    pub profile: P,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Task<S, D, K, T, C> {
    pub server: S,
    pub deployment: D,
    pub canonical: K,
    pub source: T,
    pub capture: C,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TaskRequest<B, T, U> {
    pub binding: B,
    pub task: T,
    pub caller: U,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ownership<I, L> {
    pub identity: I,
    pub labels: L,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caller<B, I, L, C, U> {
    pub binding: B,
    pub ownership: Ownership<I, L>,
    pub delegated: bool,
    pub capture: Option<C>,
    pub payload: U,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    Binding,
    Ownership,
    DelegatedCapture,
    Capture,
    Retry,
}

pub type Admission<B, S, D, K, T, C, U> = Result<TaskRequest<B, Task<S, D, K, T, C>, U>, Refusal>;

pub fn owns<I: PartialEq, L: PartialEq>(
    retained: Ownership<I, &[L]>,
    current: Ownership<I, &[L]>,
) -> bool {
    retained.identity == current.identity && contains_all(retained.labels, current.labels)
}

/// Entry multiplicity and label order do not change containment.
pub fn contains_all<L: PartialEq>(required: &[L], held: &[L]) -> bool {
    let mut i = 0;
    while i < required.len() {
        let mut j = 0;
        while j < held.len() && held[j] != required[i] {
            j += 1;
        }
        if j == held.len() {
            return false;
        }
        i += 1;
    }
    true
}

/// Ownership precedes any retained-composition lookup. Identity includes provenance.
pub fn admit_owner<B: PartialEq, I: PartialEq, L: PartialEq, S, D, K, T, C: PartialEq, U>(
    retained_binding: B,
    owner: Ownership<I, &[L]>,
    task: Task<S, D, K, T, C>,
    caller: Caller<B, I, &[L], C, U>,
) -> Admission<B, S, D, K, T, C, U> {
    if retained_binding != caller.binding {
        return Err(Refusal::Binding);
    }
    if !owns(owner, caller.ownership) {
        return Err(Refusal::Ownership);
    }
    if caller.delegated {
        let same_capture = match caller.capture {
            Some(current) => current == task.capture,
            None => false,
        };
        if !same_capture {
            return Err(Refusal::DelegatedCapture);
        }
    }
    Ok(TaskRequest {
        binding: retained_binding,
        task,
        caller: caller.payload,
    })
}

/// The successful request keeps the Task and the current caller payload, unchanged.
pub fn capture_matches<C: PartialEq, D: PartialEq>(
    retained: &C,
    actual: &C,
    deployment: &D,
    selected: Option<D>,
) -> bool {
    if retained == actual {
        match selected {
            Some(selected) => &selected == deployment,
            None => false,
        }
    } else {
        false
    }
}

/// Validate the capture while carrying the owner-admitted request onward.
pub fn capture<B, S, D: PartialEq, K, T, C: PartialEq, U>(
    request: TaskRequest<B, Task<S, D, K, T, C>, U>,
    actual_capture: C,
    actual_deployment: Option<D>,
) -> Admission<B, S, D, K, T, C, U> {
    if !capture_matches(
        &request.task.capture,
        &actual_capture,
        &request.task.deployment,
        actual_deployment,
    ) {
        return Err(Refusal::Capture);
    }
    Ok(request)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Retained<K, C, E, R> {
    pub key: K,
    pub capture: C,
    pub expires: E,
    pub record: R,
}

/// The retry key includes binding, deployment, source, ownership and authority;
/// the proposed capture is deliberately absent. Return the original record.
pub fn reuse<K: PartialEq, C, E: PartialOrd, R>(
    retained: Retained<K, C, E, R>,
    proposed_key: K,
    now: E,
) -> Result<Retained<K, C, E, R>, Refusal> {
    if retained.key == proposed_key && retained.expires > now {
        Ok(retained)
    } else {
        Err(Refusal::Retry)
    }
}
