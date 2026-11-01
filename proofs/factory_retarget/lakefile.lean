import Lake
open Lake DSL
require mathlib from git
  "https://github.com/leanprover-community/mathlib4.git" @
  "56a005855ea17e0b0958f73ec801ae140b0a1679"
package hubFactoryRetargetProof
lean_lib FactoryRetargetModel
lean_lib FactoryRetargetProof
lean_lib FactoryNativeOutputProof
lean_lib FactoryRecoveryProof
lean_lib FactoryChoiceProof
lean_lib FactoryControlFixture
lean_lib FactoryRetargetControl
lean_lib FactoryCapacityControl
lean_lib FactoryWatcherModel
lean_lib FactoryWatcherProof
lean_lib FactoryWatcherHistoryProof
