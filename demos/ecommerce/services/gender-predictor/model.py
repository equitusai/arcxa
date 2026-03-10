"""
Gender Prediction Model

Uses name-based heuristics and frequency data to predict gender.
"""

from typing import Dict, Optional


class GenderPredictor:
    """Gender prediction model based on first names."""

    def __init__(self):
        """Initialize the predictor with name frequency data."""
        self.model_version = "1.0.0"

        # Common male names with confidence scores
        self.male_names = {
            "james": 0.99, "john": 0.99, "robert": 0.99, "michael": 0.99,
            "william": 0.99, "david": 0.99, "richard": 0.99, "joseph": 0.99,
            "thomas": 0.99, "christopher": 0.99, "charles": 0.99, "daniel": 0.99,
            "matthew": 0.99, "anthony": 0.99, "mark": 0.99, "donald": 0.99,
            "steven": 0.99, "andrew": 0.99, "paul": 0.99, "joshua": 0.99,
            "kenneth": 0.98, "kevin": 0.98, "brian": 0.98, "george": 0.98,
            "timothy": 0.98, "ronald": 0.98, "edward": 0.98, "jason": 0.98,
            "jeffrey": 0.98, "ryan": 0.98, "jacob": 0.98, "gary": 0.98,
            "nicholas": 0.98, "eric": 0.98, "jonathan": 0.98, "stephen": 0.98,
            "larry": 0.97, "justin": 0.97, "scott": 0.97, "brandon": 0.97,
            "benjamin": 0.97, "samuel": 0.97, "raymond": 0.97, "gregory": 0.97,
            # Nicknames
            "jim": 0.95, "bob": 0.95, "bill": 0.95, "dick": 0.95,
            "mike": 0.95, "tom": 0.95, "chris": 0.90, "joe": 0.90,
            "dan": 0.90, "dave": 0.90, "steve": 0.90, "jeff": 0.90,
            "matt": 0.90, "tony": 0.90, "andy": 0.90, "rick": 0.90
        }

        # Common female names with confidence scores
        self.female_names = {
            "mary": 0.99, "patricia": 0.99, "jennifer": 0.99, "linda": 0.99,
            "elizabeth": 0.99, "barbara": 0.99, "susan": 0.99, "jessica": 0.99,
            "sarah": 0.99, "karen": 0.99, "nancy": 0.99, "lisa": 0.99,
            "betty": 0.99, "margaret": 0.99, "sandra": 0.99, "ashley": 0.99,
            "kimberly": 0.99, "emily": 0.99, "donna": 0.99, "michelle": 0.99,
            "dorothy": 0.98, "carol": 0.98, "amanda": 0.98, "melissa": 0.98,
            "deborah": 0.98, "stephanie": 0.98, "rebecca": 0.98, "sharon": 0.98,
            "laura": 0.98, "cynthia": 0.98, "kathleen": 0.98, "amy": 0.98,
            "angela": 0.98, "shirley": 0.98, "anna": 0.98, "brenda": 0.98,
            "pamela": 0.97, "emma": 0.97, "nicole": 0.97, "helen": 0.97,
            "samantha": 0.97, "katherine": 0.97, "christine": 0.97, "debra": 0.97,
            # Nicknames
            "beth": 0.95, "jenny": 0.95, "pat": 0.90, "sue": 0.95,
            "katie": 0.95, "kate": 0.95, "liz": 0.95, "betty": 0.95,
            "kim": 0.85, "chris": 0.70, "sam": 0.70, "alex": 0.70,
            "jamie": 0.70, "jordan": 0.70, "taylor": 0.70
        }

        # Gender-neutral names (lower confidence for both)
        self.neutral_names = {
            "alex": {"male": 0.55, "female": 0.45},
            "jordan": {"male": 0.52, "female": 0.48},
            "taylor": {"male": 0.48, "female": 0.52},
            "jamie": {"male": 0.45, "female": 0.55},
            "casey": {"male": 0.50, "female": 0.50},
            "morgan": {"male": 0.45, "female": 0.55},
            "riley": {"male": 0.48, "female": 0.52},
            "avery": {"male": 0.42, "female": 0.58},
            "quinn": {"male": 0.51, "female": 0.49},
            "drew": {"male": 0.60, "female": 0.40},
            "sam": {"male": 0.55, "female": 0.45},
            "chris": {"male": 0.58, "female": 0.42}
        }

        # Name ending patterns
        self.male_endings = ["son", "er", "ard", "ert", "ton", "den"]
        self.female_endings = ["a", "ie", "y", "lyn", "ette", "elle"]

    def normalize_name(self, name: str) -> str:
        """Normalize name for lookup."""
        if not name:
            return ""
        return name.lower().strip()

    def predict(self, first_name: str) -> Dict[str, any]:
        """
        Predict gender from first name.

        Returns:
            Dict with gender, confidence, and optional alternative prediction
        """
        norm_name = self.normalize_name(first_name)

        if not norm_name:
            return {
                "gender": "unknown",
                "confidence": 0.0,
                "alternative_gender": None,
                "alternative_confidence": None
            }

        # Check neutral names first
        if norm_name in self.neutral_names:
            scores = self.neutral_names[norm_name]
            if scores["male"] > scores["female"]:
                return {
                    "gender": "male",
                    "confidence": round(scores["male"], 3),
                    "alternative_gender": "female",
                    "alternative_confidence": round(scores["female"], 3)
                }
            else:
                return {
                    "gender": "female",
                    "confidence": round(scores["female"], 3),
                    "alternative_gender": "male",
                    "alternative_confidence": round(scores["male"], 3)
                }

        # Check male names
        if norm_name in self.male_names:
            return {
                "gender": "male",
                "confidence": round(self.male_names[norm_name], 3),
                "alternative_gender": None,
                "alternative_confidence": None
            }

        # Check female names
        if norm_name in self.female_names:
            return {
                "gender": "female",
                "confidence": round(self.female_names[norm_name], 3),
                "alternative_gender": None,
                "alternative_confidence": None
            }

        # Use pattern matching based on name endings
        confidence = 0.65  # Lower confidence for pattern-based predictions

        # Check male patterns
        for ending in self.male_endings:
            if norm_name.endswith(ending):
                return {
                    "gender": "male",
                    "confidence": confidence,
                    "alternative_gender": "female",
                    "alternative_confidence": round(1 - confidence, 3)
                }

        # Check female patterns
        for ending in self.female_endings:
            if norm_name.endswith(ending):
                return {
                    "gender": "female",
                    "confidence": confidence,
                    "alternative_gender": "male",
                    "alternative_confidence": round(1 - confidence, 3)
                }

        # Unknown - cannot make a prediction
        return {
            "gender": "unknown",
            "confidence": 0.0,
            "alternative_gender": None,
            "alternative_confidence": None
        }

    def get_model_version(self) -> str:
        """Get the model version."""
        return self.model_version
