
from pydantic import BaseModel, Extra


class SpotifyObject(BaseModel):
    class Config:
        extra = Extra.allow
