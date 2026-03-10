"""
Duplicate Detection Model

Uses fuzzy matching algorithms to detect duplicate customer records.
"""

from typing import Dict, List, Any, Optional
import re
from difflib import SequenceMatcher


class DuplicateDetector:
    """Duplicate detection model using fuzzy matching."""

    def __init__(self):
        """Initialize the detector with scoring weights."""
        self.field_weights = {
            "email": 0.30,
            "phone": 0.25,
            "name": 0.25,
            "address": 0.20
        }

    def normalize_string(self, s: Optional[str]) -> str:
        """Normalize a string for comparison."""
        if not s:
            return ""
        # Remove extra whitespace, convert to lowercase
        s = " ".join(s.lower().split())
        # Remove punctuation
        s = re.sub(r'[^\w\s]', '', s)
        return s

    def normalize_phone(self, phone: Optional[str]) -> str:
        """Normalize phone number by removing formatting."""
        if not phone:
            return ""
        # Keep only digits
        return re.sub(r'\D', '', phone)

    def normalize_email(self, email: Optional[str]) -> str:
        """Normalize email address."""
        if not email:
            return ""
        return email.lower().strip()

    def levenshtein_distance(self, s1: str, s2: str) -> int:
        """Calculate Levenshtein distance between two strings."""
        if len(s1) < len(s2):
            return self.levenshtein_distance(s2, s1)

        if len(s2) == 0:
            return len(s1)

        previous_row = range(len(s2) + 1)
        for i, c1 in enumerate(s1):
            current_row = [i + 1]
            for j, c2 in enumerate(s2):
                insertions = previous_row[j + 1] + 1
                deletions = current_row[j] + 1
                substitutions = previous_row[j] + (c1 != c2)
                current_row.append(min(insertions, deletions, substitutions))
            previous_row = current_row

        return previous_row[-1]

    def similarity_ratio(self, s1: str, s2: str) -> float:
        """Calculate similarity ratio (0-1) between two strings."""
        if not s1 and not s2:
            return 1.0
        if not s1 or not s2:
            return 0.0

        matcher = SequenceMatcher(None, s1, s2)
        return matcher.ratio()

    def compare_emails(self, email1: Optional[str], email2: Optional[str]) -> Dict[str, Any]:
        """Compare two email addresses."""
        norm1 = self.normalize_email(email1)
        norm2 = self.normalize_email(email2)

        if not norm1 or not norm2:
            return {"score": 0.0, "reason": "missing_email"}

        if norm1 == norm2:
            return {"score": 1.0, "reason": "exact_match"}

        # Check if one is a variation of the other
        similarity = self.similarity_ratio(norm1, norm2)

        if similarity > 0.9:
            return {"score": similarity, "reason": "very_similar"}
        elif similarity > 0.7:
            return {"score": similarity, "reason": "similar"}
        else:
            return {"score": similarity, "reason": "different"}

    def compare_phones(self, phone1: Optional[str], phone2: Optional[str]) -> Dict[str, Any]:
        """Compare two phone numbers."""
        norm1 = self.normalize_phone(phone1)
        norm2 = self.normalize_phone(phone2)

        if not norm1 or not norm2:
            return {"score": 0.0, "reason": "missing_phone"}

        if norm1 == norm2:
            return {"score": 1.0, "reason": "exact_match"}

        # Check for partial matches (last 7 digits)
        if len(norm1) >= 7 and len(norm2) >= 7:
            if norm1[-7:] == norm2[-7:]:
                return {"score": 0.9, "reason": "same_local_number"}

        return {"score": 0.0, "reason": "different"}

    def compare_names(self, first1: Optional[str], last1: Optional[str],
                     first2: Optional[str], last2: Optional[str]) -> Dict[str, Any]:
        """Compare two name pairs."""
        norm_first1 = self.normalize_string(first1)
        norm_last1 = self.normalize_string(last1)
        norm_first2 = self.normalize_string(first2)
        norm_last2 = self.normalize_string(last2)

        if not (norm_first1 and norm_last1 and norm_first2 and norm_last2):
            return {"score": 0.0, "reason": "missing_name"}

        # Check last name similarity (more important)
        last_similarity = self.similarity_ratio(norm_last1, norm_last2)

        if last_similarity < 0.7:
            return {"score": 0.0, "reason": "different_last_name"}

        # Check first name
        first_similarity = self.similarity_ratio(norm_first1, norm_first2)

        # Check for nickname/initial matches
        is_nickname = (
            (norm_first1 and norm_first2 and norm_first1[0] == norm_first2[0]) or
            (norm_first1 in norm_first2) or
            (norm_first2 in norm_first1)
        )

        if last_similarity == 1.0 and first_similarity == 1.0:
            return {"score": 1.0, "reason": "exact_match"}
        elif last_similarity >= 0.9 and first_similarity >= 0.9:
            return {"score": 0.95, "reason": "very_similar"}
        elif last_similarity >= 0.9 and is_nickname:
            return {"score": 0.85, "reason": "possible_nickname"}
        elif last_similarity >= 0.85 and first_similarity >= 0.7:
            return {"score": 0.8, "reason": "similar_with_typo"}
        else:
            avg_similarity = (last_similarity + first_similarity) / 2
            return {"score": avg_similarity, "reason": "somewhat_similar"}

    def compare_addresses(self, street1: Optional[str], city1: Optional[str], state1: Optional[str], zip1: Optional[str],
                         street2: Optional[str], city2: Optional[str], state2: Optional[str], zip2: Optional[str]) -> Dict[str, Any]:
        """Compare two addresses."""
        norm_street1 = self.normalize_string(street1)
        norm_street2 = self.normalize_string(street2)
        norm_city1 = self.normalize_string(city1)
        norm_city2 = self.normalize_string(city2)
        norm_state1 = self.normalize_string(state1)
        norm_state2 = self.normalize_string(state2)
        norm_zip1 = self.normalize_string(zip1)
        norm_zip2 = self.normalize_string(zip2)

        # Missing address data
        if not all([norm_street1, norm_city1, norm_state1]) or not all([norm_street2, norm_city2, norm_state2]):
            return {"score": 0.0, "reason": "missing_address"}

        # State must match
        if norm_state1 != norm_state2:
            return {"score": 0.0, "reason": "different_state"}

        # Check ZIP code
        zip_match = norm_zip1 and norm_zip2 and norm_zip1[:5] == norm_zip2[:5]

        # Check city similarity
        city_similarity = self.similarity_ratio(norm_city1, norm_city2)

        # Check street similarity
        street_similarity = self.similarity_ratio(norm_street1, norm_street2)

        if zip_match and city_similarity > 0.9 and street_similarity > 0.9:
            return {"score": 1.0, "reason": "exact_match"}
        elif zip_match and city_similarity > 0.8 and street_similarity > 0.7:
            return {"score": 0.85, "reason": "same_zip_similar_address"}
        elif city_similarity > 0.9 and street_similarity > 0.8:
            return {"score": 0.75, "reason": "similar_address_different_zip"}
        else:
            avg_similarity = (city_similarity + street_similarity) / 2
            return {"score": avg_similarity * 0.5, "reason": "partially_similar"}

    def compare_records(self, record1: Dict[str, Any], record2: Dict[str, Any]) -> Dict[str, Any]:
        """Compare two customer records and return detailed similarity analysis."""

        # Compare individual fields
        email_comparison = self.compare_emails(
            record1.get("email"),
            record2.get("email")
        )

        phone_comparison = self.compare_phones(
            record1.get("phone"),
            record2.get("phone")
        )

        name_comparison = self.compare_names(
            record1.get("first_name"),
            record1.get("last_name"),
            record2.get("first_name"),
            record2.get("last_name")
        )

        address_comparison = self.compare_addresses(
            record1.get("street"),
            record1.get("city"),
            record1.get("state"),
            record1.get("zip"),
            record2.get("street"),
            record2.get("city"),
            record2.get("state"),
            record2.get("zip")
        )

        # Calculate weighted overall confidence
        overall_confidence = (
            email_comparison["score"] * self.field_weights["email"] +
            phone_comparison["score"] * self.field_weights["phone"] +
            name_comparison["score"] * self.field_weights["name"] +
            address_comparison["score"] * self.field_weights["address"]
        )

        # Determine match reasons
        match_reasons = []
        if email_comparison["score"] >= 0.9:
            match_reasons.append(f"email_{email_comparison['reason']}")
        if phone_comparison["score"] >= 0.9:
            match_reasons.append(f"phone_{phone_comparison['reason']}")
        if name_comparison["score"] >= 0.8:
            match_reasons.append(f"name_{name_comparison['reason']}")
        if address_comparison["score"] >= 0.75:
            match_reasons.append(f"address_{address_comparison['reason']}")

        # Determine recommendation
        if overall_confidence >= 0.85:
            recommendation = "high_confidence_duplicate"
        elif overall_confidence >= 0.70:
            recommendation = "probable_duplicate"
        elif overall_confidence >= 0.50:
            recommendation = "possible_duplicate"
        else:
            recommendation = "unlikely_duplicate"

        return {
            "overall_confidence": round(overall_confidence, 3),
            "match_reasons": match_reasons,
            "field_scores": {
                "email": round(email_comparison["score"], 3),
                "phone": round(phone_comparison["score"], 3),
                "name": round(name_comparison["score"], 3),
                "address": round(address_comparison["score"], 3)
            },
            "recommendation": recommendation
        }

    def find_duplicates_in_batch(self, records: List[Dict[str, Any]], threshold: float = 0.7) -> List[Dict[str, Any]]:
        """Find all duplicate pairs in a batch of records."""
        duplicates = []

        for i in range(len(records)):
            for j in range(i + 1, len(records)):
                result = self.compare_records(records[i], records[j])

                if result["overall_confidence"] >= threshold:
                    duplicates.append({
                        "customer1_id": records[i]["customer_id"],
                        "customer2_id": records[j]["customer_id"],
                        "overall_confidence": result["overall_confidence"],
                        "match_reasons": result["match_reasons"],
                        "field_scores": result["field_scores"],
                        "recommendation": result["recommendation"]
                    })

        # Sort by confidence descending
        duplicates.sort(key=lambda x: x["overall_confidence"], reverse=True)

        return duplicates

    def suggest_merge(self, record1: Dict[str, Any], record2: Dict[str, Any]) -> Dict[str, Any]:
        """Suggest which fields to keep when merging two records."""
        merged = {}

        # For each field, keep the most complete/reliable value
        for field in ["first_name", "last_name", "email", "phone", "street", "city", "state", "zip"]:
            val1 = record1.get(field)
            val2 = record2.get(field)

            if val1 and not val2:
                merged[field] = val1
            elif val2 and not val1:
                merged[field] = val2
            elif val1 and val2:
                # Keep the longer/more complete value
                if len(str(val1)) >= len(str(val2)):
                    merged[field] = val1
                else:
                    merged[field] = val2
            else:
                merged[field] = None

        # Keep the earlier customer_id
        merged["customer_id"] = min(record1["customer_id"], record2["customer_id"])

        return merged
