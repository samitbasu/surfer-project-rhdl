import surfer


def name() -> str:
    return "Hexadecimal (Python)"


def basic_translate(num_bits: int, value: str):
    try:
        h = hex(int(value))[2:]
        return h.zfill(num_bits // 4), surfer.ValueKind.Normal()
    except ValueError:
        return value, surfer.ValueKind.Warn()
