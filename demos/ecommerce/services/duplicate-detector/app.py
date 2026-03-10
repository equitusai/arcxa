"""
Duplicate Detection ML Service

This service provides confidence scores for potential duplicate customer records
using fuzzy matching algorithms (Levenshtein distance, Jaro-Winkler, etc.)
"""

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from typing import List, Optional, Dict, Any
import uvicorn
from datetime import datetime

from model import DuplicateDetector

app = FastAPI(
    title="Duplicate Detection Service",
    description="ML service for detecting duplicate customer records with confidence scores",
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

# Initialize the duplicate detector
detector = DuplicateDetector()


# Request/Response Models
class CustomerRecord(BaseModel):
    customer_id: str
    first_name: Optional[str] = None
    last_name: Optional[str] = None
    email: Optional[str] = None
    phone: Optional[str] = None
    street: Optional[str] = None
    city: Optional[str] = None
    state: Optional[str] = None
    zip: Optional[str] = None

    class Config:
        json_schema_extra = {
            "example": {
                "customer_id": "CUST000123",
                "first_name": "John",
                "last_name": "Smith",
                "email": "john.smith@example.com",
                "phone": "(555) 123-4567",
                "street": "123 Main St",
                "city": "New York",
                "state": "NY",
                "zip": "10001"
            }
        }


class DuplicatePair(BaseModel):
    customer1: CustomerRecord
    customer2: CustomerRecord


class DuplicateScore(BaseModel):
    customer1_id: str
    customer2_id: str
    overall_confidence: float
    match_reasons: List[str]
    field_scores: Dict[str, float]
    recommendation: str


class BatchDetectionRequest(BaseModel):
    records: List[CustomerRecord]
    threshold: Optional[float] = 0.7


class BatchDetectionResponse(BaseModel):
    duplicates: List[DuplicateScore]
    total_comparisons: int
    processing_time_ms: float


@app.get("/")
async def root():
    """Health check endpoint."""
    return {
        "service": "Duplicate Detection Service",
        "status": "healthy",
        "version": "1.0.0",
        "timestamp": datetime.utcnow().isoformat()
    }


@app.get("/health")
async def health():
    """Detailed health check."""
    return {
        "status": "healthy",
        "model_loaded": detector is not None,
        "timestamp": datetime.utcnow().isoformat()
    }


@app.post("/api/v1/detect-duplicate", response_model=DuplicateScore)
async def detect_duplicate(pair: DuplicatePair):
    """
    Detect if two customer records are duplicates.

    Returns a confidence score and detailed match information.
    """
    try:
        result = detector.compare_records(
            pair.customer1.dict(),
            pair.customer2.dict()
        )

        return DuplicateScore(
            customer1_id=pair.customer1.customer_id,
            customer2_id=pair.customer2.customer_id,
            overall_confidence=result["overall_confidence"],
            match_reasons=result["match_reasons"],
            field_scores=result["field_scores"],
            recommendation=result["recommendation"]
        )

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error detecting duplicates: {str(e)}")


@app.post("/api/v1/batch-detect", response_model=BatchDetectionResponse)
async def batch_detect(request: BatchDetectionRequest):
    """
    Detect duplicates across a batch of customer records.

    Performs pairwise comparison and returns all pairs above the threshold.
    """
    start_time = datetime.utcnow()

    try:
        records = [r.dict() for r in request.records]
        duplicates = detector.find_duplicates_in_batch(
            records,
            threshold=request.threshold
        )

        # Calculate processing time
        processing_time = (datetime.utcnow() - start_time).total_seconds() * 1000

        # Convert results to response format
        duplicate_scores = [
            DuplicateScore(
                customer1_id=dup["customer1_id"],
                customer2_id=dup["customer2_id"],
                overall_confidence=dup["overall_confidence"],
                match_reasons=dup["match_reasons"],
                field_scores=dup["field_scores"],
                recommendation=dup["recommendation"]
            )
            for dup in duplicates
        ]

        return BatchDetectionResponse(
            duplicates=duplicate_scores,
            total_comparisons=len(records) * (len(records) - 1) // 2,
            processing_time_ms=processing_time
        )

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error in batch detection: {str(e)}")


@app.post("/api/v1/merge-recommendation")
async def merge_recommendation(pair: DuplicatePair):
    """
    Get a recommendation for merging two customer records.

    Returns the confidence score and suggested master record fields.
    """
    try:
        comparison = detector.compare_records(
            pair.customer1.dict(),
            pair.customer2.dict()
        )

        merged_record = detector.suggest_merge(
            pair.customer1.dict(),
            pair.customer2.dict()
        )

        return {
            "confidence": comparison["overall_confidence"],
            "recommendation": comparison["recommendation"],
            "match_reasons": comparison["match_reasons"],
            "suggested_master_record": merged_record,
            "merge_strategy": "keep_most_complete"
        }

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error generating merge recommendation: {str(e)}")


if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=8001)
