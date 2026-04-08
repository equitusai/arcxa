#!/usr/bin/env python3
"""Generate deterministic synthetic data for the ArcXA ML pipeline demo."""

from __future__ import annotations

import csv
import json
from collections import defaultdict
from dataclasses import dataclass
from datetime import date, timedelta
from pathlib import Path
from typing import Any, Dict, Iterable, List

import pyarrow as pa
import pyarrow.parquet as pq


ROOT = Path(__file__).resolve().parent
DATA_DIR = ROOT / "data"
POSTGRES_INIT_DIR = ROOT / "postgres-init"
SQL_PATH = POSTGRES_INIT_DIR / "01-seed.sql"
SUPPORT_CSV_PATH = DATA_DIR / "support_tickets.csv"
USAGE_PARQUET_PATH = DATA_DIR / "product_usage.parquet"
EXPECTED_METRICS_PATH = DATA_DIR / "expected_metrics.json"

TOTAL_CUSTOMERS = 3200

FIRST_NAMES = [
    "Ava",
    "Noah",
    "Mia",
    "Liam",
    "Emma",
    "Elijah",
    "Sophia",
    "Lucas",
    "Isabella",
    "Mason",
    "Amelia",
    "Ethan",
    "Harper",
    "Logan",
    "Evelyn",
    "James",
]
LAST_NAMES = [
    "Nguyen",
    "Patel",
    "Smith",
    "Johnson",
    "Garcia",
    "Khan",
    "Muller",
    "Chen",
    "Wilson",
    "Brown",
    "Davis",
    "Martin",
]
COUNTRIES = ["us", "ca", "gb", "de", "au", "fr"]
SEGMENTS = ["enterprise", "midmarket", "smb"]
PLAN_TIERS = ["starter", "growth", "pro", "enterprise"]
MARKETING_SOURCES = ["referral", "paid_search", "webinar", "partner", "field_event"]
TICKET_STATUSES = ["open", "pending", "resolved", "closed"]
TICKET_PRIORITIES = ["low", "medium", "high", "critical"]
SUPPORT_CHANNELS = ["email", "chat", "phone", "web"]
SUPPORT_TEAMS = ["platform", "success", "billing", "security"]
DEPLOYMENT_REGIONS = ["na", "emea", "apac"]


@dataclass(frozen=True)
class CustomerRecord:
    customer_id: str
    customer_email: str
    full_name: str
    country_code: str | None
    segment: str | None
    plan_tier: str | None
    monthly_revenue_usd: float
    account_status: str
    marketing_source: str | None
    signup_date: str
    last_contract_renewal: str


def canonical_email(idx: int) -> str:
    return f"customer{idx:04d}@demo-ml.example"


def sample_indices() -> list[int]:
    return [8, 10, 11, 13, 20, 23, 31, 66]


def normalize_email(raw: Any) -> str:
    return str(raw or "").strip().lower()


def normalize_upper(raw: Any, default: str | None = None) -> str | None:
    value = raw
    if value in (None, ""):
        value = default
    if value in (None, ""):
        return None
    return str(value).strip().upper()


def normalize_lower(raw: Any, default: str | None = None) -> str | None:
    value = raw
    if value in (None, ""):
        value = default
    if value in (None, ""):
        return None
    return str(value).strip().lower()


def format_float(value: float, decimals: int) -> float:
    return round(float(value), decimals)


def customer_source_rows() -> list[dict[str, Any]]:
    base_signup = date(2022, 1, 1)
    base_renewal = date(2025, 1, 1)
    rows: list[dict[str, Any]] = []

    for idx in range(1, TOTAL_CUSTOMERS + 1):
        first = FIRST_NAMES[(idx - 1) % len(FIRST_NAMES)]
        last = LAST_NAMES[((idx - 1) * 5) % len(LAST_NAMES)]
        full_name = f"{first} {last}"
        email = canonical_email(idx)
        country = COUNTRIES[(idx - 1) % len(COUNTRIES)]
        segment = SEGMENTS[(idx - 1) % len(SEGMENTS)]
        plan_tier = PLAN_TIERS[(idx * 3) % len(PLAN_TIERS)]
        marketing_source = MARKETING_SOURCES[(idx * 7) % len(MARKETING_SOURCES)]
        revenue = 180 + ((idx * 37) % 1900) + (((idx * 13) % 100) / 1000)
        account_status = "at_risk" if idx % 9 == 0 else "active"
        signup = base_signup + timedelta(days=(idx * 7) % 1000)
        renewal = base_renewal + timedelta(days=(idx * 11) % 420)

        canonical = {
            "customer_id": f"C{idx:05d}",
            "customer_email": f" {email.upper()} " if idx % 16 == 0 else email,
            "full_name": f" {full_name} " if idx % 27 == 0 else full_name,
            "country_code": f" {country} ",
            "segment": None if idx % 23 == 0 else f" {segment} ",
            "plan_tier": None if idx % 31 == 0 else f" {plan_tier} ",
            "monthly_revenue_usd": format_float(revenue, 3),
            "account_status": f" {account_status} ",
            "marketing_source": None
            if idx % 29 == 0
            else (marketing_source.upper() if idx % 13 == 0 else marketing_source),
            "signup_date": signup.isoformat(),
            "last_contract_renewal": renewal.isoformat(),
        }
        rows.append(canonical)

        if idx % 11 == 0:
            rows.append(
                {
                    "customer_id": f"C{idx:05d}_DUP",
                    "customer_email": f"  {email.upper()}  ",
                    "full_name": f"  {full_name.upper()}  ",
                    "country_code": country.lower(),
                    "segment": None if idx % 23 == 0 else segment.lower(),
                    "plan_tier": None if idx % 31 == 0 else plan_tier.lower(),
                    "monthly_revenue_usd": format_float(revenue, 3),
                    "account_status": f"{account_status.upper()} ",
                    "marketing_source": None
                    if idx % 29 == 0
                    else marketing_source.upper(),
                    "signup_date": signup.isoformat(),
                    "last_contract_renewal": renewal.isoformat(),
                }
            )

    rows.sort(key=lambda row: row["customer_id"])
    return rows


def support_source_rows() -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    base_opened = date(2026, 1, 1)

    for idx in range(1, TOTAL_CUSTOMERS + 1):
        if idx % 5 == 0:
            continue

        ticket_sequences = [1]
        if idx % 3 == 0:
            ticket_sequences.append(2)
        if idx % 7 == 0:
            ticket_sequences.append(3)

        for seq in ticket_sequences:
            email = canonical_email(idx)
            opened = base_opened + timedelta(days=(idx * 2 + seq) % 90)
            status = TICKET_STATUSES[(idx + seq) % len(TICKET_STATUSES)]
            priority = TICKET_PRIORITIES[(idx + seq * 2) % len(TICKET_PRIORITIES)]
            csat = 2.6 + (((idx * 7) + (seq * 11)) % 24) / 10

            row = {
                "ticket_id": f"T{idx:05d}-{seq}",
                "customer_email": f" {email.upper()} " if seq % 2 == 0 else email,
                "ticket_status": None if idx % 19 == 0 and seq == 1 else f" {status} ",
                "priority": None if idx % 17 == 0 and seq == 1 else f" {priority} ",
                "csat_score": None if idx % 8 == 0 and seq == 1 else format_float(csat, 2),
                "opened_date": opened.isoformat(),
                "channel": SUPPORT_CHANNELS[(idx + seq) % len(SUPPORT_CHANNELS)],
                "agent_team": SUPPORT_TEAMS[(idx + seq * 3) % len(SUPPORT_TEAMS)],
                "resolution_hours": format_float(4 + ((idx + seq * 13) % 96) / 2, 2),
                "first_response_hours": format_float(0.5 + ((idx + seq * 5) % 36) / 4, 2),
                "escalation_count": (idx + seq) % 3,
            }
            rows.append(row)

            if idx % 13 == 0 and seq == 1:
                rows.append(dict(row))

    return rows


def usage_source_rows() -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    base_snapshot = date(2026, 3, 31)

    for idx in range(1, TOTAL_CUSTOMERS + 1):
        if idx % 4 == 0:
            continue

        sequences = [1]
        if idx % 6 == 0:
            sequences.append(2)
        if idx % 14 == 0:
            sequences.append(3)

        for seq in sequences:
            email = canonical_email(idx)
            active_days = (idx * 3 + seq * 5) % 31
            product_events = 5 + ((idx * 11 + seq * 17) % 120)
            adoption = ((idx * 7 + seq * 3) % 101) / 100

            row = {
                "usage_record_id": f"U{idx:05d}-{seq}",
                "customer_email": email if seq % 2 == 1 else f" {email.upper()} ",
                "snapshot_date": (base_snapshot - timedelta(days=seq - 1)).isoformat(),
                "active_days_30d": None if idx % 10 == 0 and seq == 1 else active_days,
                "product_events_30d": None
                if idx % 15 == 0 and seq == 1
                else product_events,
                "feature_adoption_score": None
                if idx % 13 == 0 and seq == 1
                else format_float(adoption, 2),
                "avg_session_minutes_30d": format_float(
                    6 + ((idx * 13 + seq * 7) % 150) / 3,
                    2,
                ),
                "api_calls_30d": 50 + ((idx * 19 + seq * 23) % 500),
                "licensed_seats": 1 + (idx % 50),
                "deployment_region": DEPLOYMENT_REGIONS[(idx + seq) % len(DEPLOYMENT_REGIONS)],
            }
            rows.append(row)

            if idx % 17 == 0 and seq == 1:
                rows.append(dict(row))

    return rows


def dedup_first(rows: Iterable[dict[str, Any]], key_field: str) -> list[dict[str, Any]]:
    seen: set[str] = set()
    deduped: list[dict[str, Any]] = []
    for row in rows:
        key = normalize_email(row[key_field]) if key_field == "customer_email" else str(row[key_field])
        if key in seen:
            continue
        seen.add(key)
        deduped.append(row)
    return deduped


def curated_customer_rows(source_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    curated: list[dict[str, Any]] = []
    seen_emails: set[str] = set()
    for row in source_rows:
        email = normalize_email(row["customer_email"])
        if email in seen_emails:
            continue
        seen_emails.add(email)
        curated.append(
            {
                "customer_id": row["customer_id"],
                "customer_email": email,
                "full_name": str(row["full_name"] or "").strip(),
                "country_code": normalize_upper(row["country_code"]),
                "segment": normalize_upper(row["segment"], "UNKNOWN_SEGMENT"),
                "plan_tier": normalize_upper(row["plan_tier"], "STANDARD"),
                "monthly_revenue_usd": format_float(row["monthly_revenue_usd"], 2),
                "account_status": normalize_upper(row["account_status"]),
                "marketing_source": normalize_lower(row["marketing_source"], "unknown"),
                "signup_date": row["signup_date"],
                "last_contract_renewal": row["last_contract_renewal"],
            }
        )
    return curated


def normalized_support_rows(source_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    normalized: list[dict[str, Any]] = []
    for row in source_rows:
        normalized.append(
            {
                "ticket_id": row["ticket_id"],
                "customer_email": normalize_email(row["customer_email"]),
                "ticket_status": normalize_upper(row["ticket_status"], "OPEN"),
                "priority": normalize_upper(row["priority"], "MEDIUM"),
                "csat_score": format_float(
                    row["csat_score"] if row["csat_score"] not in (None, "") else 3.5,
                    2,
                ),
            }
        )
    return normalized


def aggregated_support_features(source_rows: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    deduped = dedup_first(normalized_support_rows(source_rows), "ticket_id")
    grouped: dict[str, list[float]] = defaultdict(list)
    for row in deduped:
        grouped[row["customer_email"]].append(float(row["csat_score"]))

    features: dict[str, dict[str, Any]] = {}
    for email, csat_values in grouped.items():
        features[email] = {
            "customer_email": email,
            "ticket_count_90d": len(csat_values),
            "avg_csat_90d": format_float(sum(csat_values) / len(csat_values), 2),
        }
    return features


def normalized_usage_rows(source_rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    normalized: list[dict[str, Any]] = []
    for row in source_rows:
        normalized.append(
            {
                "usage_record_id": row["usage_record_id"],
                "customer_email": normalize_email(row["customer_email"]),
                "active_days_30d": format_float(
                    row["active_days_30d"] if row["active_days_30d"] not in (None, "") else 0,
                    0,
                ),
                "product_events_30d": format_float(
                    row["product_events_30d"]
                    if row["product_events_30d"] not in (None, "")
                    else 0,
                    0,
                ),
                "feature_adoption_score": format_float(
                    row["feature_adoption_score"]
                    if row["feature_adoption_score"] not in (None, "")
                    else 0,
                    2,
                ),
            }
        )
    return normalized


def aggregated_usage_features(source_rows: list[dict[str, Any]]) -> dict[str, dict[str, Any]]:
    deduped = dedup_first(normalized_usage_rows(source_rows), "usage_record_id")
    grouped: dict[str, list[dict[str, Any]]] = defaultdict(list)
    for row in deduped:
        grouped[row["customer_email"]].append(row)

    features: dict[str, dict[str, Any]] = {}
    for email, rows in grouped.items():
        features[email] = {
            "customer_email": email,
            "avg_active_days_30d": format_float(
                sum(float(row["active_days_30d"]) for row in rows) / len(rows), 2
            ),
            "total_product_events_30d": int(
                sum(float(row["product_events_30d"]) for row in rows)
            ),
            "feature_adoption_score": format_float(
                max(float(row["feature_adoption_score"]) for row in rows), 2
            ),
        }
    return features


def assembled_training_rows(
    curated_customers: list[dict[str, Any]],
    support_features: dict[str, dict[str, Any]],
    usage_features: dict[str, dict[str, Any]],
) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for customer in curated_customers:
        email = customer["customer_email"]
        support = support_features.get(email)
        usage = usage_features.get(email)
        support_available = 1 if support else 0
        usage_available = 1 if usage else 0
        usable = 1 if support_available and usage_available else 0
        avg_active = format_float(usage["avg_active_days_30d"], 2) if usage else 0.0
        total_events = int(usage["total_product_events_30d"]) if usage else 0
        feature_adoption = (
            format_float(usage["feature_adoption_score"], 2) if usage else 0.0
        )
        ticket_count = int(support["ticket_count_90d"]) if support else 0
        avg_csat = format_float(support["avg_csat_90d"], 2) if support else 0.0

        churn_label = 1 if (
            customer["account_status"] == "AT_RISK"
            or avg_active < 8
            or (ticket_count >= 3 and avg_csat < 4)
        ) else 0

        rows.append(
            {
                "customer_id": customer["customer_id"],
                "customer_email": email,
                "full_name": customer["full_name"],
                "country_code": customer["country_code"],
                "segment": customer["segment"],
                "plan_tier": customer["plan_tier"],
                "monthly_revenue_usd": format_float(customer["monthly_revenue_usd"], 2),
                "ticket_count_90d": ticket_count,
                "avg_csat_90d": avg_csat,
                "avg_active_days_30d": avg_active,
                "total_product_events_30d": total_events,
                "feature_adoption_score": feature_adoption,
                "support_signal_available": support_available,
                "usage_signal_available": usage_available,
                "ml_sample_usable": usable,
                "churn_label": churn_label,
            }
        )
    return rows


def sql_literal(value: Any) -> str:
    if value is None:
        return "NULL"
    if isinstance(value, (int, float)):
        return str(value)
    escaped = str(value).replace("'", "''")
    return f"'{escaped}'"


def write_postgres_seed_sql(customer_rows: list[dict[str, Any]]) -> None:
    header = """CREATE SCHEMA IF NOT EXISTS ml_demo;

DROP TABLE IF EXISTS ml_demo.crm_customers;
CREATE TABLE ml_demo.crm_customers (
    customer_id TEXT PRIMARY KEY,
    customer_email TEXT NOT NULL,
    full_name TEXT NOT NULL,
    country_code TEXT,
    segment TEXT,
    plan_tier TEXT,
    monthly_revenue_usd NUMERIC(12, 3) NOT NULL,
    account_status TEXT NOT NULL,
    marketing_source TEXT,
    signup_date DATE NOT NULL,
    last_contract_renewal DATE NOT NULL
);

DROP TABLE IF EXISTS ml_demo.customer_master_curated;
CREATE TABLE ml_demo.customer_master_curated (
    customer_id TEXT,
    customer_email TEXT PRIMARY KEY,
    full_name TEXT,
    country_code TEXT,
    segment TEXT,
    plan_tier TEXT,
    monthly_revenue_usd NUMERIC(12, 2),
    account_status TEXT,
    marketing_source TEXT,
    signup_date DATE,
    last_contract_renewal DATE
);

DROP TABLE IF EXISTS ml_demo.customer_support_features;
CREATE TABLE ml_demo.customer_support_features (
    customer_email TEXT PRIMARY KEY,
    ticket_count_90d NUMERIC(12, 0),
    avg_csat_90d NUMERIC(8, 2)
);

DROP TABLE IF EXISTS ml_demo.customer_usage_features;
CREATE TABLE ml_demo.customer_usage_features (
    customer_email TEXT PRIMARY KEY,
    avg_active_days_30d NUMERIC(8, 2),
    total_product_events_30d NUMERIC(12, 0),
    feature_adoption_score NUMERIC(8, 2)
);

DROP TABLE IF EXISTS ml_demo.customer_training_features;
CREATE TABLE ml_demo.customer_training_features (
    customer_id TEXT,
    customer_email TEXT PRIMARY KEY,
    full_name TEXT,
    country_code TEXT,
    segment TEXT,
    plan_tier TEXT,
    monthly_revenue_usd NUMERIC(12, 2),
    ticket_count_90d NUMERIC(12, 0),
    avg_csat_90d NUMERIC(8, 2),
    avg_active_days_30d NUMERIC(8, 2),
    total_product_events_30d NUMERIC(12, 0),
    feature_adoption_score NUMERIC(8, 2),
    support_signal_available INTEGER,
    usage_signal_available INTEGER,
    ml_sample_usable INTEGER,
    churn_label INTEGER
);

"""

    columns = [
        "customer_id",
        "customer_email",
        "full_name",
        "country_code",
        "segment",
        "plan_tier",
        "monthly_revenue_usd",
        "account_status",
        "marketing_source",
        "signup_date",
        "last_contract_renewal",
    ]
    value_lines = []
    for row in customer_rows:
        values = ", ".join(sql_literal(row[column]) for column in columns)
        value_lines.append(f"    ({values})")

    batches: list[str] = []
    batch_size = 500
    for start in range(0, len(value_lines), batch_size):
        batch = ",\n".join(value_lines[start : start + batch_size])
        batches.append(
            "INSERT INTO ml_demo.crm_customers (\n    "
            + ",\n    ".join(columns)
            + "\n) VALUES\n"
            + batch
            + ";\n"
        )

    SQL_PATH.write_text(header + "\n".join(batches), encoding="utf-8")


def write_support_csv(rows: list[dict[str, Any]]) -> None:
    fieldnames = [
        "ticket_id",
        "customer_email",
        "ticket_status",
        "priority",
        "csat_score",
        "opened_date",
        "channel",
        "agent_team",
        "resolution_hours",
        "first_response_hours",
        "escalation_count",
    ]
    with SUPPORT_CSV_PATH.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        for row in rows:
            writer.writerow(row)


def write_usage_parquet(rows: list[dict[str, Any]]) -> None:
    table = pa.Table.from_pylist(rows)
    pq.write_table(table, USAGE_PARQUET_PATH)


def write_expected_metrics(metrics: dict[str, Any]) -> None:
    EXPECTED_METRICS_PATH.write_text(
        json.dumps(metrics, indent=2, sort_keys=True),
        encoding="utf-8",
    )


def build_expected_metrics(
    customers_source: list[dict[str, Any]],
    support_source: list[dict[str, Any]],
    usage_source: list[dict[str, Any]],
    curated_customers: list[dict[str, Any]],
    support_features: dict[str, dict[str, Any]],
    usage_features: dict[str, dict[str, Any]],
    final_rows: list[dict[str, Any]],
) -> dict[str, Any]:
    sample_rows = {
        canonical_email(idx): next(
            row for row in final_rows if row["customer_email"] == canonical_email(idx)
        )
        for idx in sample_indices()
    }

    return {
        "total_customers": TOTAL_CUSTOMERS,
        "customer_source_row_count": len(customers_source),
        "customer_duplicate_merge_count": len(customers_source) - len(curated_customers),
        "customer_source_rows_with_segment_null": sum(
            1 for row in customers_source if row["segment"] is None
        ),
        "customer_source_rows_with_plan_tier_null": sum(
            1
            for row in customers_source
            if row["plan_tier"] is None
        ),
        "customer_source_rows_with_marketing_source_null": sum(
            1
            for row in customers_source
            if row["marketing_source"] is None
        ),
        "curated_customer_count": len(curated_customers),
        "curated_customer_rows_with_segment_default": sum(
            1 for row in curated_customers if row["segment"] == "UNKNOWN_SEGMENT"
        ),
        "curated_customer_rows_with_plan_tier_default": sum(
            1 for row in curated_customers if row["plan_tier"] == "STANDARD"
        ),
        "curated_customer_rows_with_marketing_source_default": sum(
            1 for row in curated_customers if row["marketing_source"] == "unknown"
        ),
        "support_source_row_count": len(support_source),
        "support_duplicate_merge_count": len(support_source)
        - len(dedup_first(normalized_support_rows(support_source), "ticket_id")),
        "support_rows_with_status_default": sum(
            1 for row in support_source if row["ticket_status"] in (None, "")
        ),
        "support_rows_with_priority_default": sum(
            1 for row in support_source if row["priority"] in (None, "")
        ),
        "support_rows_with_csat_default": sum(
            1 for row in support_source if row["csat_score"] in (None, "")
        ),
        "support_feature_customer_count": len(support_features),
        "usage_source_row_count": len(usage_source),
        "usage_duplicate_merge_count": len(usage_source)
        - len(dedup_first(normalized_usage_rows(usage_source), "usage_record_id")),
        "usage_rows_with_active_days_default": sum(
            1 for row in usage_source if row["active_days_30d"] in (None, "")
        ),
        "usage_rows_with_product_events_default": sum(
            1 for row in usage_source if row["product_events_30d"] in (None, "")
        ),
        "usage_rows_with_feature_adoption_default": sum(
            1 for row in usage_source if row["feature_adoption_score"] in (None, "")
        ),
        "usage_feature_customer_count": len(usage_features),
        "final_feature_row_count": len(final_rows),
        "support_signal_available_count": sum(
            row["support_signal_available"] for row in final_rows
        ),
        "usage_signal_available_count": sum(
            row["usage_signal_available"] for row in final_rows
        ),
        "ml_sample_usable_count": sum(row["ml_sample_usable"] for row in final_rows),
        "ml_sample_unusable_count": sum(1 - row["ml_sample_usable"] for row in final_rows),
        "lineage_example_email": canonical_email(11),
        "sample_rows": sample_rows,
    }


def main() -> None:
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    POSTGRES_INIT_DIR.mkdir(parents=True, exist_ok=True)

    customers_source = customer_source_rows()
    support_source = support_source_rows()
    usage_source = usage_source_rows()
    curated_customers = curated_customer_rows(customers_source)
    support_features = aggregated_support_features(support_source)
    usage_features = aggregated_usage_features(usage_source)
    final_rows = assembled_training_rows(
        curated_customers,
        support_features,
        usage_features,
    )
    expected_metrics = build_expected_metrics(
        customers_source,
        support_source,
        usage_source,
        curated_customers,
        support_features,
        usage_features,
        final_rows,
    )

    write_postgres_seed_sql(customers_source)
    write_support_csv(support_source)
    write_usage_parquet(usage_source)
    write_expected_metrics(expected_metrics)

    print(
        json.dumps(
            {
                "customer_source_rows": len(customers_source),
                "support_source_rows": len(support_source),
                "usage_source_rows": len(usage_source),
                "final_feature_rows": len(final_rows),
                "ml_sample_usable_count": expected_metrics["ml_sample_usable_count"],
            },
            indent=2,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    main()
