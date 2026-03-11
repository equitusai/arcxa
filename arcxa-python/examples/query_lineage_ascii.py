#!/usr/bin/env python3
"""
Lineage Query and ASCII Visualization Script

This script queries the Graphica lineage API and displays lineage traces
using ASCII arrow notation for easy visualization in the terminal.

Features:
- Row-level lineage tracking (CSV → Transform → DB2)
- Batch lineage aggregation
- Column-level lineage tracking
- ASCII tree visualization with arrows
- Color-coded output (optional)

Requirements:
- Graphica coordinator running on localhost:8082
- Python 3.7+
- Optional: colorama for colored output (pip install colorama)

Usage:
    python query_lineage_ascii.py
    python query_lineage_ascii.py --no-color
    python query_lineage_ascii.py --batch-id BATCH123
"""

import requests
import json
import sys
import argparse
from typing import Dict, List, Any, Optional
from datetime import datetime

# Try to import colorama for colored output
try:
    from colorama import init, Fore, Style
    init(autoreset=True)
    HAS_COLOR = True
except ImportError:
    HAS_COLOR = False
    class Fore:
        GREEN = YELLOW = RED = CYAN = MAGENTA = BLUE = WHITE = RESET = ""
    class Style:
        BRIGHT = DIM = RESET_ALL = ""

# Configuration
COORDINATOR_URL = "http://localhost:8082"
API_BASE = f"{COORDINATOR_URL}/api/v1/lineage"


class LineageVisualizer:
    """Visualizes lineage data with ASCII arrows"""

    def __init__(self, use_color: bool = True):
        self.use_color = use_color and HAS_COLOR
        self.session = requests.Session()
        self.session.headers.update({"Content-Type": "application/json"})

    def color(self, text: str, color_code: str) -> str:
        """Apply color to text if colors are enabled"""
        if self.use_color:
            return f"{color_code}{text}{Style.RESET_ALL}"
        return text

    def print_header(self, text: str):
        """Print a formatted header"""
        print("\n" + "=" * 70)
        print(self.color(f"  {text}", Fore.CYAN + Style.BRIGHT))
        print("=" * 70)

    def print_row_lineage(self, row_key: str):
        """Query and display row-level lineage with ASCII arrows"""
        self.print_header(f"Row Lineage: {row_key}")

        try:
            # Query row lineage
            response = self.session.get(f"{API_BASE}/row/{row_key}", timeout=10)

            if response.status_code == 404:
                print(self.color(f"  ✗ No lineage found for row: {row_key}", Fore.RED))
                print(self.color(f"    Hint: Row key format should be 'csv:filename:row_num' or 'db:table:id'", Fore.YELLOW))
                return

            response.raise_for_status()
            lineage = response.json()

            # Display lineage chain
            print(f"\n  {self.color('Source:', Fore.GREEN)}")
            source_type = lineage.get("source_type", "unknown")
            source_id = lineage.get("source_id", "N/A")
            print(f"    {self.color('●', Fore.GREEN)} {source_type}: {source_id}")

            # Show transformations
            transformations = lineage.get("transformations", [])
            if transformations:
                print(f"\n  {self.color('Transformations:', Fore.YELLOW)}")
                for i, transform in enumerate(transformations, 1):
                    arrow = "  └──>" if i == len(transformations) else "  ├──>"
                    transform_type = transform.get("type", "unknown")
                    timestamp = transform.get("timestamp", "")
                    print(f"    {self.color(arrow, Fore.YELLOW)} {transform_type} ({timestamp})")

                    # Show transformation details
                    if transform.get("details"):
                        details = transform["details"]
                        for key, value in details.items():
                            print(f"         {self.color('•', Fore.CYAN)} {key}: {value}")

            # Show destination
            print(f"\n  {self.color('Destination:', Fore.BLUE)}")
            dest_type = lineage.get("destination_type", "unknown")
            dest_id = lineage.get("destination_id", "N/A")
            print(f"    {self.color('●', Fore.BLUE)} {dest_type}: {dest_id}")

            # Show quality metrics if available
            quality = lineage.get("quality_score")
            if quality is not None:
                quality_color = Fore.GREEN if quality > 0.8 else Fore.YELLOW if quality > 0.5 else Fore.RED
                print(f"\n  {self.color('Quality Score:', Fore.MAGENTA)} {self.color(f'{quality:.2%}', quality_color)}")

            print()

        except requests.RequestException as e:
            print(self.color(f"  ✗ Error querying lineage: {e}", Fore.RED))

    def print_row_journey(self, row_key: str):
        """Query and display row journey (end-to-end trace) with ASCII arrows"""
        self.print_header(f"Row Journey: {row_key}")

        try:
            response = self.session.get(f"{API_BASE}/row/{row_key}/journey", timeout=10)

            if response.status_code == 404:
                print(self.color(f"  ✗ No journey found for row: {row_key}", Fore.RED))
                return

            response.raise_for_status()
            journey = response.json()

            steps = journey.get("steps", [])
            if not steps:
                print(self.color("  No journey steps found", Fore.YELLOW))
                return

            print(f"\n  {self.color('Journey Steps:', Fore.CYAN)}")
            print()

            for i, step in enumerate(steps, 1):
                is_last = i == len(steps)
                connector = "  └──>" if is_last else "  ├──>"
                vertical = "       " if is_last else "  │    "

                step_name = step.get("name", f"Step {i}")
                step_type = step.get("type", "unknown")
                status = step.get("status", "unknown")

                # Color-code status
                if status == "success":
                    status_display = self.color("✓ " + status, Fore.GREEN)
                elif status == "failed":
                    status_display = self.color("✗ " + status, Fore.RED)
                else:
                    status_display = self.color("● " + status, Fore.YELLOW)

                print(f"    {self.color(connector, Fore.CYAN)} {self.color(step_name, Style.BRIGHT)} [{step_type}]")
                print(f"    {vertical}Status: {status_display}")

                # Show input/output
                if step.get("input"):
                    print(f"    {vertical}Input:  {step['input']}")
                if step.get("output"):
                    print(f"    {vertical}Output: {step['output']}")

                # Show timestamp
                if step.get("timestamp"):
                    print(f"    {vertical}Time:   {step['timestamp']}")

                # Show quality violations if any
                violations = step.get("quality_violations", [])
                if violations:
                    print(f"    {vertical}{self.color('Violations:', Fore.RED)}")
                    for violation in violations:
                        print(f"    {vertical}  • {violation.get('message', 'Unknown violation')}")

                if not is_last:
                    print(f"    {vertical}")

            # Summary
            total_time = journey.get("total_processing_time_ms")
            if total_time:
                print(f"\n  {self.color('Total Processing Time:', Fore.MAGENTA)} {total_time} ms")

            print()

        except requests.RequestException as e:
            print(self.color(f"  ✗ Error querying journey: {e}", Fore.RED))

    def print_batch_lineage(self, batch_id: str):
        """Query and display batch lineage (aggregate of multiple rows)"""
        self.print_header(f"Batch Lineage: {batch_id}")

        try:
            response = self.session.get(f"{API_BASE}/batch/{batch_id}", timeout=10)

            if response.status_code == 404:
                print(self.color(f"  ✗ No lineage found for batch: {batch_id}", Fore.RED))
                return

            response.raise_for_status()
            batch = response.json()

            # Show batch statistics
            print(f"\n  {self.color('Batch Statistics:', Fore.CYAN)}")
            total_rows = batch.get("total_rows", 0)
            successful = batch.get("successful_rows", 0)
            failed = batch.get("failed_rows", 0)

            print(f"    Total Rows:      {total_rows}")
            print(f"    {self.color('Successful:', Fore.GREEN)}     {successful}")
            print(f"    {self.color('Failed:', Fore.RED)}         {failed}")

            # Show processing pipeline
            pipeline = batch.get("pipeline", [])
            if pipeline:
                print(f"\n  {self.color('Processing Pipeline:', Fore.YELLOW)}")
                for i, stage in enumerate(pipeline, 1):
                    arrow = "  └──>" if i == len(pipeline) else "  ├──>"
                    stage_name = stage.get("name", f"Stage {i}")
                    rows_processed = stage.get("rows_processed", "N/A")
                    print(f"    {arrow} {stage_name} ({rows_processed} rows)")

            # Show sample row IDs
            row_ids = batch.get("row_ids", [])
            if row_ids:
                print(f"\n  {self.color('Sample Rows:', Fore.MAGENTA)} (showing up to 5)")
                for row_id in row_ids[:5]:
                    print(f"    • {row_id}")

            print()

        except requests.RequestException as e:
            print(self.color(f"  ✗ Error querying batch: {e}", Fore.RED))

    def print_column_lineage(self, table: str, column: str):
        """Query and display column-level lineage"""
        self.print_header(f"Column Lineage: {table}.{column}")

        try:
            response = self.session.get(f"{API_BASE}/column/{table}/{column}", timeout=10)

            if response.status_code == 404:
                print(self.color(f"  ✗ No lineage found for column: {table}.{column}", Fore.RED))
                return

            response.raise_for_status()
            col_lineage = response.json()

            # Show upstream columns
            upstream = col_lineage.get("upstream_columns", [])
            if upstream:
                print(f"\n  {self.color('Upstream Dependencies:', Fore.GREEN)}")
                for col in upstream:
                    source_table = col.get("table", "unknown")
                    source_col = col.get("column", "unknown")
                    print(f"    ┌──> {source_table}.{source_col}")
                print(f"    │")
                print(f"    └───> {self.color(f'{table}.{column}', Style.BRIGHT)}")

            # Show transformations
            transformations = col_lineage.get("transformations", [])
            if transformations:
                print(f"\n  {self.color('Transformations:', Fore.YELLOW)}")
                for transform in transformations:
                    expr = transform.get("expression", "N/A")
                    transform_type = transform.get("type", "unknown")
                    print(f"    • {transform_type}: {expr}")

            # Show downstream columns
            downstream = col_lineage.get("downstream_columns", [])
            if downstream:
                print(f"\n  {self.color('Downstream Usage:', Fore.BLUE)}")
                print(f"    {self.color(f'{table}.{column}', Style.BRIGHT)}")
                for col in downstream:
                    dest_table = col.get("table", "unknown")
                    dest_col = col.get("column", "unknown")
                    print(f"    └───> {dest_table}.{dest_col}")

            print()

        except requests.RequestException as e:
            print(self.color(f"  ✗ Error querying column lineage: {e}", Fore.RED))

    def demonstrate_lineage(self):
        """Demonstrate lineage queries with sample data"""
        print(self.color("\n╔═══════════════════════════════════════════════════════════════════╗", Fore.CYAN))
        print(self.color("║          Graphica Lineage Query Demonstration                    ║", Fore.CYAN + Style.BRIGHT))
        print(self.color("╚═══════════════════════════════════════════════════════════════════╝", Fore.CYAN))

        # Check if coordinator is running
        try:
            health = self.session.get(f"{COORDINATOR_URL}/health", timeout=5)
            health.raise_for_status()
            print(self.color(f"\n✓ Coordinator is running at {COORDINATOR_URL}", Fore.GREEN))
        except requests.RequestException as e:
            print(self.color(f"\n✗ Cannot connect to coordinator at {COORDINATOR_URL}", Fore.RED))
            print(self.color(f"  Error: {e}", Fore.RED))
            print(self.color(f"  Please ensure the coordinator is running with ./run-local.sh", Fore.YELLOW))
            return

        # Example 1: Row lineage (if data exists)
        print(self.color("\n\n[Example 1] Querying Row-Level Lineage", Fore.YELLOW + Style.BRIGHT))
        print(self.color("-" * 70, Fore.YELLOW))
        print("Trying sample row keys...")
        sample_keys = [
            "csv:healthcare_patients.csv:1",
            "db:HEALTHCARE_PATIENTS:1",
            "csv:test.csv:1"
        ]

        for key in sample_keys:
            self.print_row_lineage(key)

        # Example 2: Row journey
        print(self.color("\n\n[Example 2] Querying Row Journey (End-to-End Trace)", Fore.YELLOW + Style.BRIGHT))
        print(self.color("-" * 70, Fore.YELLOW))
        print("Trying sample row journey...")
        self.print_row_journey("csv:healthcare_patients.csv:1")

        # Example 3: Batch lineage
        print(self.color("\n\n[Example 3] Querying Batch Lineage", Fore.YELLOW + Style.BRIGHT))
        print(self.color("-" * 70, Fore.YELLOW))
        print("Trying sample batch ID...")
        self.print_batch_lineage("BATCH_001")

        # Example 4: Column lineage
        print(self.color("\n\n[Example 4] Querying Column-Level Lineage", Fore.YELLOW + Style.BRIGHT))
        print(self.color("-" * 70, Fore.YELLOW))
        print("Trying sample column...")
        self.print_column_lineage("HEALTHCARE_PATIENTS", "PATIENT_ID")

        # Summary
        print(self.color("\n\n╔═══════════════════════════════════════════════════════════════════╗", Fore.CYAN))
        print(self.color("║                    Demo Complete                                 ║", Fore.CYAN + Style.BRIGHT))
        print(self.color("╚═══════════════════════════════════════════════════════════════════╝", Fore.CYAN))

        print(self.color("\nAvailable Lineage APIs:", Fore.GREEN))
        print(f"  • Row Lineage:    {API_BASE}/row/<row_key>")
        print(f"  • Row Journey:    {API_BASE}/row/<row_key>/journey")
        print(f"  • Batch Lineage:  {API_BASE}/batch/<batch_id>")
        print(f"  • Column Lineage: {API_BASE}/column/<table>/<column>")

        print(self.color("\nTip:", Fore.YELLOW))
        print("  After running an ETL workflow, use the row keys from the output")
        print("  to query actual lineage data from your pipeline!")
        print()


def main():
    parser = argparse.ArgumentParser(description="Query and visualize Graphica lineage data")
    parser.add_argument("--no-color", action="store_true", help="Disable colored output")
    parser.add_argument("--row-key", type=str, help="Query specific row key")
    parser.add_argument("--batch-id", type=str, help="Query specific batch ID")
    parser.add_argument("--column", type=str, help="Query column lineage (format: table.column)")

    args = parser.parse_args()

    visualizer = LineageVisualizer(use_color=not args.no_color)

    # If specific queries provided, run them
    if args.row_key:
        visualizer.print_row_lineage(args.row_key)
        visualizer.print_row_journey(args.row_key)
    elif args.batch_id:
        visualizer.print_batch_lineage(args.batch_id)
    elif args.column:
        parts = args.column.split(".")
        if len(parts) == 2:
            visualizer.print_column_lineage(parts[0], parts[1])
        else:
            print(f"Error: Column format should be 'table.column', got: {args.column}")
    else:
        # Run demonstration with sample queries
        visualizer.demonstrate_lineage()


if __name__ == "__main__":
    main()
