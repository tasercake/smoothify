from fastapi import APIRouter

from server.api.api_v1.endpoints import smoothify_endpoints

api_router = APIRouter()
api_router.include_router(
    smoothify_endpoints.router, prefix="/smoothify", tags=["smoothify"]
)
