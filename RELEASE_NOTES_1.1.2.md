# ARCXA v1.1.2

Release date: 2026-05-06

## Highlights
- Hardened SAP ECC bridge execution with secret-store-backed credential resolution and rotation-aware metadata.
- Added richer ECC session lifecycle handling, including cached session reuse, explicit session close, and TTL hints.
- Tightened ECC extractor-family validation for IDoc, ODP, and generic package lanes.
- Deepened SAP verification and migration-evidence runtime documentation.

## SAP / ECC Hardening
- `sap_ecc_adapter` and `sap_ecc_rfc_bapi` now resolve bridge credentials from configured secret stores when `secret_ref` is present.
- Connector and verification metadata now capture secret provenance such as secret store, secret version, and rotation timestamps.
- ECC verification now supports stronger connection policy semantics:
  - cached session reuse
  - explicit stateful session close
  - session TTL hints
  - clearer session metadata on resulting controls

## Extractor-Family Assurance
- IDoc packages now require message identity metadata.
- ODP delta packages now require either a delta token or a complete subscriber and queue context.
- Generic extractor packages now require extractor object or context identifiers.

## Migration Evidence Graph
- The migration-evidence gateway, ingestion service, and verification service now share stronger ECC bridge auth/session behavior.
- Public-facing documentation now reflects the current SAP transport model more accurately across HANA, S/4 OData, ECC adapter, RFC/BAPI, staged export, IDoc, and ODP lanes.

## Notes
- This release continues the ARCXA Migration Evidence Graph rollout as a microservice-oriented, evidence-first architecture beside enterprise migration tooling.
