"""
Address Validation Heuristics API

This service provides confidence scores for address validation using
heuristic rules for format checking, completeness, and consistency.
"""

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from typing import List, Optional, Dict
import uvicorn
from datetime import datetime

from validator import AddressValidator

app = FastAPI(
    title="Address Validation Service",
    description="Heuristics API for validating address data quality",
    version="1.0.0"
)

# CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Initialize the address validator
validator = AddressValidator()


# Request/Response Models
class Address(BaseModel):
    street: Optional[str] = None
    city: Optional[str] = None
    state: Optional[str] = None
    zip: Optional[str] = None
    country: Optional[str] = "US"

    class Config:
        json_schema_extra = {
            "example": {
                "street": "123 Main St",
                "city": "New York",
                "state": "NY",
                "zip": "10001",
                "country": "US"
            }
        }


class ValidationRequest(BaseModel):
    customer_id: Optional[str] = None
    address: Address


class ValidationIssue(BaseModel):
    field: str
    issue_type: str
    severity: str
    message: str
    suggestion: Optional[str] = None


class ValidationResponse(BaseModel):
    customer_id: Optional[str]
    is_valid: bool
    overall_confidence: float
    quality_score: float
    issues: List[ValidationIssue]
    field_scores: Dict[str, float]
    standardized_address: Optional[Address] = None


class BatchValidationRequest(BaseModel):
    addresses: List[ValidationRequest]


class BatchValidationResponse(BaseModel):
    validations: List[ValidationResponse]
    processing_time_ms: float
    summary: Dict[str, int]


@app.get("/")
async def root():
    """Health check endpoint."""
    return {
        "service": "Address Validation Service",
        "status": "healthy",
        "version": "1.0.0",
        "timestamp": datetime.utcnow().isoformat()
    }


@app.get("/health")
async def health():
    """Detailed health check."""
    return {
        "status": "healthy",
        "validator_loaded": validator is not None,
        "supported_countries": ["US"],
        "timestamp": datetime.utcnow().isoformat()
    }


@app.post("/api/v1/validate", response_model=ValidationResponse)
async def validate_address(request: ValidationRequest):
    """
    Validate an address and return quality assessment.

    Returns validation issues, confidence score, and standardized address.
    """
    try:
        result = validator.validate(request.address.dict())

        # Convert issues to response format
        issues = [
            ValidationIssue(
                field=issue["field"],
                issue_type=issue["issue_type"],
                severity=issue["severity"],
                message=issue["message"],
                suggestion=issue.get("suggestion")
            )
            for issue in result["issues"]
        ]

        # Convert standardized address
        standardized = None
        if result.get("standardized_address"):
            standardized = Address(**result["standardized_address"])

        return ValidationResponse(
            customer_id=request.customer_id,
            is_valid=result["is_valid"],
            overall_confidence=result["overall_confidence"],
            quality_score=result["quality_score"],
            issues=issues,
            field_scores=result["field_scores"],
            standardized_address=standardized
        )

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error validating address: {str(e)}")


@app.post("/api/v1/batch-validate", response_model=BatchValidationResponse)
async def batch_validate(request: BatchValidationRequest):
    """
    Validate a batch of addresses.

    Returns validation results for all addresses in the batch.
    """
    start_time = datetime.utcnow()

    try:
        validations = []
        summary = {"valid": 0, "invalid": 0, "warning": 0}

        for addr_request in request.addresses:
            result = validator.validate(addr_request.address.dict())

            # Convert issues
            issues = [
                ValidationIssue(
                    field=issue["field"],
                    issue_type=issue["issue_type"],
                    severity=issue["severity"],
                    message=issue["message"],
                    suggestion=issue.get("suggestion")
                )
                for issue in result["issues"]
            ]

            # Convert standardized address
            standardized = None
            if result.get("standardized_address"):
                standardized = Address(**result["standardized_address"])

            validation = ValidationResponse(
                customer_id=addr_request.customer_id,
                is_valid=result["is_valid"],
                overall_confidence=result["overall_confidence"],
                quality_score=result["quality_score"],
                issues=issues,
                field_scores=result["field_scores"],
                standardized_address=standardized
            )

            validations.append(validation)

            # Update summary
            if result["is_valid"]:
                summary["valid"] += 1
            else:
                has_error = any(i["severity"] == "error" for i in result["issues"])
                if has_error:
                    summary["invalid"] += 1
                else:
                    summary["warning"] += 1

        # Calculate processing time
        processing_time = (datetime.utcnow() - start_time).total_seconds() * 1000

        return BatchValidationResponse(
            validations=validations,
            processing_time_ms=processing_time,
            summary=summary
        )

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error in batch validation: {str(e)}")


@app.post("/api/v1/standardize")
async def standardize_address(address: Address):
    """
    Standardize an address to a consistent format.

    Returns standardized version of the address.
    """
    try:
        standardized = validator.standardize(address.dict())

        return {
            "original": address.dict(),
            "standardized": standardized,
            "changes_made": validator.get_standardization_changes(address.dict(), standardized)
        }

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error standardizing address: {str(e)}")


@app.get("/api/v1/validation-rules")
async def get_validation_rules():
    """Get information about validation rules."""
    return {
        "rules": [
            {
                "name": "completeness",
                "description": "Check if all required fields are present",
                "severity": "error"
            },
            {
                "name": "state_validation",
                "description": "Validate state code against known states",
                "severity": "error"
            },
            {
                "name": "zip_format",
                "description": "Validate ZIP code format (5 digits or 5+4)",
                "severity": "error"
            },
            {
                "name": "street_format",
                "description": "Check street address has number and name",
                "severity": "warning"
            },
            {
                "name": "city_format",
                "description": "Check city name format and length",
                "severity": "warning"
            },
            {
                "name": "consistency",
                "description": "Check ZIP code matches city/state",
                "severity": "warning"
            }
        ]
    }


if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=8003)
