from typing import Any

from fastapi import APIRouter, Depends
from pydantic.networks import EmailStr

from server import models, schemas
from server.api import deps
from server.core.celery_app import celery_app
from server.utils import send_test_email

router = APIRouter()


@router.post("/test-celery/", response_model=schemas.Msg, status_code=201)
def test_celery(
    msg: schemas.Msg,
    current_user: models.User = Depends(deps.get_current_active_superuser),
) -> Any:
    """
    Test Celery worker.
    """
    celery_app.send_task("app.worker.test_celery", args=[msg.msg])
    return {"msg": "Word received"}


@router.post("/test-email/", response_model=schemas.Msg, status_code=201)
def test_email(
    email_to: EmailStr,
    current_user: models.User = Depends(deps.get_current_active_superuser),
) -> Any:
    """
    Test emails.
    """
    send_test_email(email_to=email_to)
    return {"msg": "Test email sent"}
