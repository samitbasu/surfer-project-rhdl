# from spade import BitTranslator, SpadeType
# from surfer import TranslationResult, SignalInfo

# def fields_from_type(type: SpadeType) -> SignalInfo:
#     result = SignalInfo()
# 
#     for (field_name, field_type) in type.fields():
#         subfields = fields_from_type(field_type)
#         result.with_field((field_name, subfields))
# 
#     return result

class pytest:
    def __init__(self):
        print("Loading spade translation extension")
        # self.translator = BitTranslator("/home/frans/Documents/fpga/spadev/build/spade_types.ron")

    def translates(self):
        return True

    def translate(self, name: str, value: str):
        # result = self.translator.translate_value(name, value)
        # if result is not None:
        #     return TranslationResult(result)
        # else:
        return "test"

    def signal_info(self, name: str):
        # type = self.translator.type_of(name)

        # if type is not None:
        #     return fields_from_type(type)
        return None



