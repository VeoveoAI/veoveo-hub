# Console Session Projection

## Standards And Protocols

| Boundary | Supported profile |
|---|---|
| HTTP, RFC 9110 | Authenticated `GET /console-api/{profile}/session`, JSON, `no-store` |
| Veoveo OAuth and Work Context | Existing gateway authentication, exact profile audience, current session family and invocation authority |
| JSON Schema 2020-12 | Closed `ConsoleBootstrap`, branding and identity DTOs owned by `veoveo-gateway-contract` |

The session response opens the Console without installation-inventory authority.
It contains current identity, one current tenant, Work Context and public branding.
It never loads principal inventories, Tasks, servers or control-plane secrets.
`canReadInstallation` reflects the current catalog's `AdminRead` decision against
the Gateway target. It is a presentation hint; administrative handlers independently
authorize and audit their requests. Bootstrap does not execute an administrative action.
`canReadAudit` reports the actor's audit scope independently of installation inventory
access. The [audit handlers](../admin/console/audit/DESIGN.md) authorize every read.

The existing authentication middleware selects the profile from `console-api` and
establishes current subject authority before projection. An unknown profile returns
404. `presentation.rs` uses authenticated display metadata, or the compact subject
when the token carries none.
