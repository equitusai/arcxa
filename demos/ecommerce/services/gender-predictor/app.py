"""
Gender Prediction ML Service

This service provides gender predictions based on first names with confidence scores.
Uses a probabilistic model based on name frequency data.
"""

from fastapi import FastAPI, HTTPException
from fastapi.middleware.cors import CORSMiddleware
from pydantic import BaseModel
from typing import List, Optional
import uvicorn
from datetime import datetime

from model import GenderPredictor

app = FastAPI(
    title="Gender Prediction Service",
    description="ML service for predicting gender from customer names",
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

# Initialize the gender predictor
predictor = GenderPredictor()


# Request/Response Models
class PredictionRequest(BaseModel):
    first_name: str
    customer_id: Optional[str] = None

    class Config:
        json_schema_extra = {
            "example": {
                "first_name": "Jennifer",
                "customer_id": "CUST000123"
            }
        }


class PredictionResponse(BaseModel):
    customer_id: Optional[str]
    first_name: str
    predicted_gender: str
    confidence: float
    alternative_gender: Optional[str] = None
    alternative_confidence: Optional[float] = None
    model_version: str


class BatchPredictionRequest(BaseModel):
    records: List[PredictionRequest]


class BatchPredictionResponse(BaseModel):
    predictions: List[PredictionResponse]
    processing_time_ms: float


@app.get("/")
async def root():
    """Health check endpoint."""
    return {
        "service": "Gender Prediction Service",
        "status": "healthy",
        "version": "1.0.0",
        "timestamp": datetime.utcnow().isoformat()
    }


@app.get("/health")
async def health():
    """Detailed health check."""
    return {
        "status": "healthy",
        "model_loaded": predictor is not None,
        "model_version": predictor.get_model_version(),
        "timestamp": datetime.utcnow().isoformat()
    }


@app.post("/api/v1/predict", response_model=PredictionResponse)
async def predict_gender(request: PredictionRequest):
    """
    Predict gender from a first name.

    Returns predicted gender with confidence score.
    """
    try:
        result = predictor.predict(request.first_name)

        return PredictionResponse(
            customer_id=request.customer_id,
            first_name=request.first_name,
            predicted_gender=result["gender"],
            confidence=result["confidence"],
            alternative_gender=result.get("alternative_gender"),
            alternative_confidence=result.get("alternative_confidence"),
            model_version=predictor.get_model_version()
        )

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error predicting gender: {str(e)}")


@app.post("/api/v1/batch-predict", response_model=BatchPredictionResponse)
async def batch_predict(request: BatchPredictionRequest):
    """
    Predict gender for a batch of names.

    Returns predictions for all names in the batch.
    """
    start_time = datetime.utcnow()

    try:
        predictions = []

        for record in request.records:
            result = predictor.predict(record.first_name)

            predictions.append(PredictionResponse(
                customer_id=record.customer_id,
                first_name=record.first_name,
                predicted_gender=result["gender"],
                confidence=result["confidence"],
                alternative_gender=result.get("alternative_gender"),
                alternative_confidence=result.get("alternative_confidence"),
                model_version=predictor.get_model_version()
            ))

        # Calculate processing time
        processing_time = (datetime.utcnow() - start_time).total_seconds() * 1000

        return BatchPredictionResponse(
            predictions=predictions,
            processing_time_ms=processing_time
        )

    except Exception as e:
        raise HTTPException(status_code=500, detail=f"Error in batch prediction: {str(e)}")


@app.get("/api/v1/model-info")
async def model_info():
    """Get information about the prediction model."""
    return {
        "model_name": "Gender Predictor",
        "model_version": predictor.get_model_version(),
        "supported_genders": ["male", "female", "unknown"],
        "confidence_range": [0.0, 1.0],
        "training_data_source": "US Census Bureau name frequency data",
        "last_updated": "2024-01-01"
    }


if __name__ == "__main__":
    uvicorn.run(app, host="0.0.0.0", port=8002)
