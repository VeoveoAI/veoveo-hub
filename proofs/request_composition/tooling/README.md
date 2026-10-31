# Aeneas discriminant instance names

This is a manual proof-tool dependency patch, not a Hub runtime change or a new
proof runner. It applies to Aeneas revision
`fdebf20d78e8007bb92fc17068f17fc2e6cfbbbf`, with Lean 4.35.0.

The pinned `discriminant` attribute gives its helper function the enum's qualified
name but emits an unnamed typeclass instance. Separately compiled generated types
with the same basename can therefore export the same instance name. Importing both
modules fails before their proofs are checked. Request composition encountered this
with `LocalResult` in the request and current-role generated models.

The [checked receipt](../evidence/discriminant-tooling-checkpoint.json) records
the stock failure, patched success and successful rebuilds of both unchanged
proof packages. This repairs a proof import; it adds no new Hub contract coverage.

[The two-line patch](qualified-discriminant-instance.patch) names each instance
`<qualified-enum>.instDiscriminant`. It preserves the instance's target, binders,
helper function and discriminant values. No generated output is edited.

## Reproduce the regression

Use two separate temporary trees, one stock and one patched. Each has this layout:

```text
proofs/
  aeneas/                    # private checkout of the exact revision above
  discriminant_collision/   # copy of this directory's fixture
```

Copy source files only, excluding every existing `.lake` directory. Each checkout
and fixture needs its own `.lake/build`. Do not patch a shared
`.lake/packages/aeneas` checkout. Reusing external dependency caches does not make
those caches read-only; avoid sharing artifacts for the tool being changed.

In the patched checkout only, apply the versioned patch with native Git:

```sh
git apply --check /path/to/qualified-discriminant-instance.patch
git apply /path/to/qualified-discriminant-instance.patch
```

From each `discriminant_collision` directory, use its committed toolchain and
relative dependency declaration. Resolve dependencies once with `lake update`,
then run:

```sh
LAKE_NO_UPDATE=1 lake build CollisionLeft CollisionRight
LAKE_NO_UPDATE=1 lake build CollisionTogether
```

Both separate modules must compile. The stock joint import must fail specifically
at the duplicate `instDiscriminantLocalResultIsize` declaration. With the patch,
the joint import must succeed, resolve both generic enum instances and check all
six default/explicit discriminant equations. Its executable axiom audit rejects
unexpected axioms, including `sorryAx`. A dependency download or other compile
failure is not evidence of the intended regression.

## Check the actual proof packages

Keep the committed baseline packages and manifests unchanged. In the patched
temporary tree, add source-only copies as siblings, again excluding `.lake`:

```text
proofs/
  aeneas/
  authority/
  current_roles/
  sdk_authorization/        # SDK 239622d12c08b771cc5e813fe3e846385d8d04ad,
                            # proofs/authorization only
  request_composition/
```

In these temporary `lakefile.lean` files only:

1. Add `require aeneas from "../aeneas/backends/lean"` to request composition.
2. Replace the Git Aeneas requirement in current roles, authority and SDK
   authorization with that same relative path requirement.
3. Replace current roles' Git SDK requirement with
   `require sdkAuthorizationProof from "../sdk_authorization"`.

Resolve the corresponding temporary manifests to those local packages. Preserve
the committed revisions of every other dependency; retain the effective manifest
and configuration diff with the check evidence. The qualification receipt records
the explicit path records used for the checked run. These path overrides and the
versioned patch together identify the tool used; the baseline manifest alone does
not identify the patched tool.

Run `LAKE_NO_UPDATE=1 lake build` from current roles and then from request
composition. From request composition, also run:

```sh
LAKE_NO_UPDATE=1 lake env lean tooling/ActualImports.lean
```

The latter imports the actual colliding generated modules together through their
existing proof modules and resolves their distinct discriminant instances. It is
an import regression, not a completed selected-registration theorem. Keep source
hashes before and after the checks, with tool revision, patch hash, effective
configuration, command, exit status and log hash. Build outputs and private
dependency checkouts remain temporary.
