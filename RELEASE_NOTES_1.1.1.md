# ARCXA v1.1.1

## Highlights
- Deepened SAP migration-evidence support with clearer, transport-specific lanes for SAP HANA, SAP S/4HANA, and SAP ECC.
- Added bounded SAP ECC RFC/BAPI live-read verification for targeted dispute resolution and spot checks.
- Added SAP IDoc / extractor package ingestion with checksum and row-count validation for higher-assurance evidence capture.
- Hardened SAP HANA result decoding and verification so typed comparisons, projection validation, and reconciliation metadata are more trustworthy.

## Migration Evidence
- `sap_hana_sql` now benefits from shared HANA runtime handling, typed result coercion, and stronger verification control metadata.
- `sap_s4_odata` continues to support `$metadata`-driven projection validation and paged rowset verification.
- `sap_ecc_adapter` remains the bounded live ECC adapter lane.
- `sap_ecc_rfc_bapi` is now available as a narrower ECC live bridge for targeted reads.
- `sap_ecc_staged_export` and `sap_idoc_extractor_package` provide controlled ECC evidence-ingest lanes.

## Assurance Improvements
- Verification controls now preserve richer transport-specific metadata for requested fields, missing projections, pagination, and typed comparison outcomes.
- ECC evidence packages fail closed on integrity mismatches instead of silently downgrading assurance.
- Public docs now describe the actual SAP transport posture more accurately.
