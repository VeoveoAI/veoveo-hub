-- Run from the private request_composition package after rebuilding both
-- unchanged packages with the explicitly overridden private Aeneas checkout.
import SnapshotAssemblyProof
import CollectionProof

open Aeneas Aeneas.Std

#check SnapshotAssemblyProof.http_new_exact
#check IteratorProof.filter_next_correct
#check RequestCompositionHub.registration.snapshot.RegistrationSnapshot
#check veoveo_hub.registration.snapshot.RegistrationSnapshot
#synth Discriminant (RequestCompositionHub.chrono.offset.LocalResult Nat) Isize
#synth Discriminant (veoveo_hub.chrono.offset.LocalResult Nat) Isize
