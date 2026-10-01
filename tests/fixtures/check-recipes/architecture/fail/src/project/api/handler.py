from project import (
    db,
)

customer = db.customers.find("42")
