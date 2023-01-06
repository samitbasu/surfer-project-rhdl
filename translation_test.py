import spade
import surfer

class pytest:
    def __init__(self):
        print("Loading spade translation extension")
        self.translator = spade.BitTranslator("/home/frans/Documents/fpga/spadev/build/spade_types.ron")

    def translates(self):
        return True

    def translate(self, name: str, value: str):
        result = self.translator.translate_value(name, value)
        if result is not None:
            return surfer.TranslationResult(result)
        else:
            return value

    def signal_info(self, name: str):
        pass

