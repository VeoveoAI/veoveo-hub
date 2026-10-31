import Lake
open Lake DSL

-- Explicit manual validation layout: this fixture and the private pinned
-- Aeneas checkout are siblings. Never point this at a shared .lake cache.
require aeneas from "../aeneas/backends/lean"
package discriminantCollisionRegression
lean_lib CollisionLeft
lean_lib CollisionRight
@[default_target]
lean_lib CollisionTogether
