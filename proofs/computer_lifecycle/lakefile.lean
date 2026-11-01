import Lake
open Lake DSL
require mathlib from git
  "https://github.com/leanprover-community/mathlib4.git" @
  "56a005855ea17e0b0958f73ec801ae140b0a1679"
package computerLifecycleProof
-- Manual named targets only;no default/CI target or extraction claim.
lean_lib ComputerLifecycleModel
lean_lib ComputerLifecycleSpecification
lean_lib ComputerLifecycleWitnesses
lean_lib ComputerLifecycleProof
