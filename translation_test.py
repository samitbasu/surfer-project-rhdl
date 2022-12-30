class pytest:
    def __init__(self):
        self.counter = 0

    def translates(self):
        return True

    def translate(self, name: str, value: str):
        val_lower = value.lower()
        if "x" in val_lower:
            return "UNDEF"
        elif "z" in val_lower:
            return "HIGHIMP"
        else:
            self.counter += 1
            return f"{self.counter}"
