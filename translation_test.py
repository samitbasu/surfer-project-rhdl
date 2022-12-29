from spade import Spade

def translates():
    return True

def translate(name: str, value: str):
    val_lower = value.lower()
    if "x" in val_lower:
        return "UNDEF"
    elif "z" in val_lower:
        return "HIGHIMP"
    else:
        return "other"
