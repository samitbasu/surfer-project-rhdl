import surfer

translators = ["HexTranslator"]


class HexTranslator(surfer.BasicTranslator):
    name = "Hexadecimal (Python)"

    @staticmethod
    def basic_translate(num_bits: int, value: str):
        try:
            h = hex(int(value))[2:]
            return f"0x{h.zfill(num_bits // 4)}", surfer.ValueKind.Normal()
        except ValueError:
            return value, surfer.ValueKind.Warn()
