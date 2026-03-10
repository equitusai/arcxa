"""
Address Validator

Heuristic-based address validation for data quality assessment.
"""

import re
from typing import Dict, List, Optional, Any


class AddressValidator:
    """Address validator using heuristic rules."""

    def __init__(self):
        """Initialize the validator with reference data."""

        # US state codes
        self.valid_states = {
            "AL", "AK", "AZ", "AR", "CA", "CO", "CT", "DE", "FL", "GA",
            "HI", "ID", "IL", "IN", "IA", "KS", "KY", "LA", "ME", "MD",
            "MA", "MI", "MN", "MS", "MO", "MT", "NE", "NV", "NH", "NJ",
            "NM", "NY", "NC", "ND", "OH", "OK", "OR", "PA", "RI", "SC",
            "SD", "TN", "TX", "UT", "VT", "VA", "WA", "WV", "WI", "WY"
        }

        # State name to code mapping
        self.state_names = {
            "alabama": "AL", "alaska": "AK", "arizona": "AZ", "arkansas": "AR",
            "california": "CA", "colorado": "CO", "connecticut": "CT", "delaware": "DE",
            "florida": "FL", "georgia": "GA", "hawaii": "HI", "idaho": "ID",
            "illinois": "IL", "indiana": "IN", "iowa": "IA", "kansas": "KS",
            "kentucky": "KY", "louisiana": "LA", "maine": "ME", "maryland": "MD",
            "massachusetts": "MA", "michigan": "MI", "minnesota": "MN", "mississippi": "MS",
            "missouri": "MO", "montana": "MT", "nebraska": "NE", "nevada": "NV",
            "new hampshire": "NH", "new jersey": "NJ", "new mexico": "NM", "new york": "NY",
            "north carolina": "NC", "north dakota": "ND", "ohio": "OH", "oklahoma": "OK",
            "oregon": "OR", "pennsylvania": "PA", "rhode island": "RI", "south carolina": "SC",
            "south dakota": "SD", "tennessee": "TN", "texas": "TX", "utah": "UT",
            "vermont": "VT", "virginia": "VA", "washington": "WA", "west virginia": "WV",
            "wisconsin": "WI", "wyoming": "WY"
        }

        # Street suffixes
        self.street_suffixes = {
            "ST", "STREET", "AVE", "AVENUE", "RD", "ROAD", "BLVD", "BOULEVARD",
            "DR", "DRIVE", "LN", "LANE", "CT", "COURT", "PL", "PLACE",
            "WAY", "CIR", "CIRCLE", "TER", "TERRACE", "PKWY", "PARKWAY"
        }

        # ZIP code ranges by state (simplified - first 3 digits)
        self.zip_ranges = {
            "NY": ["100", "101", "102", "103", "104", "105", "106", "107", "108", "109",
                   "110", "111", "112", "113", "114", "115", "116", "117", "118", "119"],
            "CA": ["900", "901", "902", "903", "904", "905", "906", "907", "908",
                   "910", "911", "912", "913", "914", "915", "916", "917", "918", "919",
                   "920", "921", "922", "923", "924", "925", "926", "927", "928", "929",
                   "930", "931", "932", "933", "934", "935", "936", "937", "938", "939",
                   "940", "941", "942", "943", "944", "945", "946", "947", "948", "949",
                   "950", "951", "952", "953", "954", "955", "956", "957", "958", "959", "961"],
            "TX": ["750", "751", "752", "753", "754", "755", "756", "757", "758", "759",
                   "760", "761", "762", "763", "764", "765", "766", "767", "768", "769",
                   "770", "771", "772", "773", "774", "775", "776", "777", "778", "779",
                   "780", "781", "782", "783", "784", "785", "786", "787", "788", "789",
                   "790", "791", "792", "793", "794", "795", "796", "797", "798", "799",
                   "885"]
        }

    def normalize_string(self, s: Optional[str]) -> str:
        """Normalize a string."""
        if not s:
            return ""
        return " ".join(s.strip().split())

    def validate_completeness(self, address: Dict[str, Any]) -> List[Dict[str, Any]]:
        """Check if address has all required fields."""
        issues = []

        required_fields = ["street", "city", "state", "zip"]

        for field in required_fields:
            value = address.get(field)
            if not value or not str(value).strip():
                issues.append({
                    "field": field,
                    "issue_type": "missing_value",
                    "severity": "error",
                    "message": f"{field.capitalize()} is required but missing"
                })

        return issues

    def validate_state(self, address: Dict[str, Any]) -> List[Dict[str, Any]]:
        """Validate state code."""
        issues = []
        state = address.get("state")

        if not state:
            return issues

        state_upper = state.upper().strip()

        # Check if it's a valid state code
        if state_upper not in self.valid_states:
            # Check if it's a state name
            state_lower = state.lower().strip()
            if state_lower in self.state_names:
                issues.append({
                    "field": "state",
                    "issue_type": "format_error",
                    "severity": "warning",
                    "message": f"State should use 2-letter code instead of full name",
                    "suggestion": self.state_names[state_lower]
                })
            else:
                issues.append({
                    "field": "state",
                    "issue_type": "invalid_value",
                    "severity": "error",
                    "message": f"Invalid state code: {state}"
                })

        return issues

    def validate_zip(self, address: Dict[str, Any]) -> List[Dict[str, Any]]:
        """Validate ZIP code format."""
        issues = []
        zip_code = address.get("zip")

        if not zip_code:
            return issues

        zip_str = str(zip_code).strip()

        # Valid formats: 12345 or 12345-6789
        if not re.match(r'^\d{5}(-\d{4})?$', zip_str):
            issues.append({
                "field": "zip",
                "issue_type": "format_error",
                "severity": "error",
                "message": f"Invalid ZIP code format: {zip_code}. Expected 12345 or 12345-6789"
            })

        return issues

    def validate_street(self, address: Dict[str, Any]) -> List[Dict[str, Any]]:
        """Validate street address format."""
        issues = []
        street = address.get("street")

        if not street:
            return issues

        street_normalized = self.normalize_string(street)

        # Check if street has a number
        if not re.search(r'\d', street_normalized):
            issues.append({
                "field": "street",
                "issue_type": "format_warning",
                "severity": "warning",
                "message": "Street address should typically include a number"
            })

        # Check if street has a suffix
        street_upper = street_normalized.upper()
        has_suffix = any(suffix in street_upper for suffix in self.street_suffixes)

        if not has_suffix:
            issues.append({
                "field": "street",
                "issue_type": "format_warning",
                "severity": "info",
                "message": "Street address may be missing a suffix (St, Ave, Rd, etc.)"
            })

        return issues

    def validate_city(self, address: Dict[str, Any]) -> List[Dict[str, Any]]:
        """Validate city format."""
        issues = []
        city = address.get("city")

        if not city:
            return issues

        city_normalized = self.normalize_string(city)

        # Check city length
        if len(city_normalized) < 2:
            issues.append({
                "field": "city",
                "issue_type": "format_error",
                "severity": "error",
                "message": "City name is too short"
            })

        # Check for numbers in city name (unusual)
        if re.search(r'\d', city_normalized):
            issues.append({
                "field": "city",
                "issue_type": "format_warning",
                "severity": "warning",
                "message": "City name contains numbers which is unusual"
            })

        return issues

    def validate_consistency(self, address: Dict[str, Any]) -> List[Dict[str, Any]]:
        """Validate consistency between ZIP code and state."""
        issues = []

        state = address.get("state", "").upper().strip()
        zip_code = address.get("zip", "").strip()

        if not state or not zip_code or state not in self.zip_ranges:
            return issues

        # Extract first 3 digits of ZIP
        zip_prefix = zip_code[:3]

        # Check if ZIP prefix matches state
        if zip_prefix not in self.zip_ranges.get(state, []):
            issues.append({
                "field": "zip",
                "issue_type": "consistency_error",
                "severity": "warning",
                "message": f"ZIP code {zip_code} may not match state {state}"
            })

        return issues

    def validate(self, address: Dict[str, Any]) -> Dict[str, Any]:
        """
        Validate an address and return detailed results.

        Returns:
            Dict with validation results including issues, scores, and standardized address
        """
        issues = []

        # Run all validation checks
        issues.extend(self.validate_completeness(address))
        issues.extend(self.validate_state(address))
        issues.extend(self.validate_zip(address))
        issues.extend(self.validate_street(address))
        issues.extend(self.validate_city(address))
        issues.extend(self.validate_consistency(address))

        # Calculate field scores
        field_scores = {}
        fields = ["street", "city", "state", "zip"]

        for field in fields:
            field_issues = [i for i in issues if i["field"] == field]
            error_count = len([i for i in field_issues if i["severity"] == "error"])
            warning_count = len([i for i in field_issues if i["severity"] == "warning"])

            # Start with 1.0 and subtract based on issues
            score = 1.0
            score -= error_count * 0.5
            score -= warning_count * 0.2
            score = max(0.0, min(1.0, score))

            field_scores[field] = round(score, 3)

        # Calculate overall scores
        quality_score = sum(field_scores.values()) / len(field_scores) if field_scores else 0.0

        # Has any errors?
        has_errors = any(i["severity"] == "error" for i in issues)
        is_valid = not has_errors

        # Calculate confidence (reduced by warnings)
        confidence = quality_score
        if not is_valid:
            confidence *= 0.5

        # Generate standardized address
        standardized = self.standardize(address)

        return {
            "is_valid": is_valid,
            "overall_confidence": round(confidence, 3),
            "quality_score": round(quality_score, 3),
            "issues": issues,
            "field_scores": field_scores,
            "standardized_address": standardized
        }

    def standardize(self, address: Dict[str, Any]) -> Dict[str, Any]:
        """Standardize address to consistent format."""
        standardized = {}

        # Normalize street
        street = address.get("street")
        if street:
            street_norm = self.normalize_string(street)
            # Standardize suffixes
            for suffix in ["Street", "Avenue", "Road", "Boulevard", "Drive", "Lane"]:
                suffix_abbr = {"Street": "St", "Avenue": "Ave", "Road": "Rd",
                              "Boulevard": "Blvd", "Drive": "Dr", "Lane": "Ln"}
                street_norm = re.sub(rf'\b{suffix}\b', suffix_abbr.get(suffix, suffix),
                                    street_norm, flags=re.IGNORECASE)
            standardized["street"] = street_norm.title()
        else:
            standardized["street"] = None

        # Normalize city
        city = address.get("city")
        if city:
            standardized["city"] = self.normalize_string(city).title()
        else:
            standardized["city"] = None

        # Normalize state (convert to 2-letter code)
        state = address.get("state")
        if state:
            state_upper = state.upper().strip()
            if state_upper in self.valid_states:
                standardized["state"] = state_upper
            else:
                # Try to convert from name
                state_lower = state.lower().strip()
                standardized["state"] = self.state_names.get(state_lower, state_upper)
        else:
            standardized["state"] = None

        # Normalize ZIP (ensure 5-digit format)
        zip_code = address.get("zip")
        if zip_code:
            zip_str = str(zip_code).strip()
            # Keep only digits
            zip_digits = re.sub(r'\D', '', zip_str)
            if len(zip_digits) >= 5:
                standardized["zip"] = zip_digits[:5]
            else:
                standardized["zip"] = zip_str
        else:
            standardized["zip"] = None

        # Keep country
        standardized["country"] = address.get("country", "US")

        return standardized

    def get_standardization_changes(self, original: Dict[str, Any],
                                   standardized: Dict[str, Any]) -> List[str]:
        """Get list of changes made during standardization."""
        changes = []

        for field in ["street", "city", "state", "zip"]:
            orig = original.get(field)
            std = standardized.get(field)

            if orig != std and orig is not None and std is not None:
                changes.append(f"{field}: '{orig}' -> '{std}'")

        return changes
