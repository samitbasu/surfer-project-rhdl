from typing import ClassVar


class BasicTranslator:
    name: ClassVar[str]

    @staticmethod
    def basic_translate(self, num_bits: int, value: str) -> tuple[str, ValueKind]: ...


class ValueKind:
    @classmethod
    def Normal(cls): ...

    @classmethod
    def Undef(cls): ...

    @classmethod
    def HighImp(cls): ...

    @classmethod
    def Custom(cls, color: tuple[int, int, int, int]): ...

    @classmethod
    def Warn(cls): ...

    @classmethod
    def DontCare(cls): ...

    @classmethod
    def Weak(cls): ...
