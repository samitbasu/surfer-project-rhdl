import spade

class pytest:
    def __init__(self):
        print("Loading spade translation extension")
        self.translator = spade.BitTranslator("/home/frans/Documents/fpga/spadev/build/spade_types.ron")

    def translates(self):
        return True

    def translate(self, name: str, value: str):
        result = self.translator.translate_value(name, value)
        print(f"Translating '{name}'' -> {result}")
        if result is not None:
            return result
        else:
            return value
