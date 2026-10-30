import Lake
open Lake DSL
require hubCurrentRolesProof from "../current_roles"
package refreshTransactionsProof
@[default_target]
lean_lib RefreshTransactionModel

@[default_target]
lean_lib RefreshTransactionProof
