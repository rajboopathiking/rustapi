from rustapi import APIRouter

router = APIRouter(prefix="/api")

@router.get("/items")
def get_items():
    return {"items": [{"id": i, "name": f"Item {i}"} for i in range(10)]}