import uvicorn

if __name__ == "__main__":
    uvicorn.run("server.main:app", reload=True)
