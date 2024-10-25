use super::{
    check_single_wordlength, check_vector_variable, no_of_digits, TranslationPreference, ValueKind,
};
use crate::wave_container::{ScopeId, VarId, VariableMeta};

use color_eyre::Result;
use instruction_decoder::Decoder;
use surfer_translation_types::{BasicTranslator, VariableValue};

pub struct InstructionTranslator {
    pub name: String,
    pub decoder: Decoder,
    pub num_bits: u64,
}

impl BasicTranslator<VarId, ScopeId> for InstructionTranslator {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn basic_translate(&self, num_bits: u64, value: &VariableValue) -> (String, ValueKind) {
        let u64_value = match value {
            VariableValue::BigUint(v) => v.to_u64_digits().last().cloned(),
            VariableValue::String(s) => match check_vector_variable(s) {
                Some(v) => return v,
                None => u64::from_str_radix(s, 2).ok(),
            },
        }
        .unwrap_or(0);

        match self
            .decoder
            .decode_from_i64(u64_value as i64, num_bits as usize)
        {
            Ok(iform) => (iform, ValueKind::Normal),
            _ => (
                format!(
                    "UNKNOWN INSN ({:#0width$x})",
                    u64_value,
                    width = no_of_digits(num_bits, 4) + 2
                ),
                ValueKind::Warn,
            ),
        }
    }

    fn translates(&self, variable: &VariableMeta) -> Result<TranslationPreference> {
        check_single_wordlength(variable.num_bits, self.num_bits as u32)
    }
}

pub fn new_rv32_translator() -> InstructionTranslator {
    InstructionTranslator {
        name: "RV32".into(),
        decoder: Decoder::new(&[
            include_str!("../../../instruction-decoder/toml/RV32I.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32M.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32A.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32F.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32_Zbb.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32_Zbkb.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32_Zbs.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32_Zknd.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32_Zkne.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32_Zfa.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32_Zicsr.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32C-lower.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32_Zcb-lower.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32_Zcf-lower.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV32_Zacas.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zcd-lower.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zfh.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zba.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zbc.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zbkc.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zbkx.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zfh.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zknh.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zksed.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zksh.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zawrs.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zicond.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zifencei.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zicbo.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zimop.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zihintntl.toml").to_string(),
        ])
        .expect("Can't build RV32 decoder"),
        num_bits: 32,
    }
}

pub fn new_rv64_translator() -> InstructionTranslator {
    InstructionTranslator {
        name: "RV64".into(),
        decoder: Decoder::new(&[
            include_str!("../../../instruction-decoder/toml/RV64I.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64M.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64A.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64D.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64_Zbb.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64_Zbkb.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64_Zbs.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64_Zbkb.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64_Zbs.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64_Zknd.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64_Zkne.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64_Zacas.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64_Zfa.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64C-lower.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64_Zcb-lower.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV64_Zcd-lower.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RVV.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zvbb.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zvbc.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zvkg.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zvkned.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zvknha.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zvknhb.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zvksed.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zvksh.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zcd-lower.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zfh.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zba.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zbc.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zbkc.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zbkx.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zfh.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zknh.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zksed.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zksh.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zawrs.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zicond.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zifencei.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zicbo.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zimop.toml").to_string(),
            include_str!("../../../instruction-decoder/toml/RV_Zihintntl.toml").to_string(),
        ])
        .expect("Can't build RV64 decoder"),
        num_bits: 32,
    }
}

pub fn new_mips_translator() -> InstructionTranslator {
    InstructionTranslator {
        name: "MIPS".into(),
        decoder: Decoder::new(&[
            include_str!("../../../instruction-decoder/toml/mips.toml").to_string()
        ])
        .expect("Can't build mips decoder"),
        num_bits: 32,
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn riscv_from_bigunit() {
        let rv32_translator = new_rv32_translator();
        let rv64_translator = new_rv64_translator();
        assert_eq!(
            rv32_translator
                .basic_translate(32, &VariableValue::BigUint(1u32.into()))
                .0,
            "c.nop"
        );
        assert_eq!(
            rv32_translator
                .basic_translate(32, &VariableValue::BigUint(0b1000000010011111u32.into()))
                .0,
            "UNKNOWN INSN (0x0000809f)"
        );
        assert_eq!(
            rv32_translator
                .basic_translate(
                    32,
                    &VariableValue::BigUint(0b1000_0001_0011_0101_0000_0101_1001_0011_u32.into())
                )
                .0,
            "addi a1, a0, -2029"
        );
        assert_eq!(
            rv64_translator
                .basic_translate(32, &VariableValue::BigUint(1u32.into()))
                .0,
            "c.nop"
        );
        assert_eq!(
            rv64_translator
                .basic_translate(32, &VariableValue::BigUint(0b1000000010011111u32.into()))
                .0,
            "UNKNOWN INSN (0x0000809f)"
        );
        assert_eq!(
            rv64_translator
                .basic_translate(
                    32,
                    &VariableValue::BigUint(0b1000_0001_0011_0101_0000_0101_1001_0011_u32.into())
                )
                .0,
            "addi a1, a0, -2029"
        );
    }
    #[test]
    fn riscv_from_string() {
        let rv32_translator = new_rv32_translator();
        assert_eq!(
            rv32_translator
                .basic_translate(32, &VariableValue::String("1".to_owned()))
                .0,
            "c.nop"
        );
        assert_eq!(
            rv32_translator
                .basic_translate(
                    32,
                    &VariableValue::String("01001000100010001000100011111111".to_owned())
                )
                .0,
            "UNKNOWN INSN (0x488888ff)"
        );
        assert_eq!(
            rv32_translator
                .basic_translate(
                    32,
                    &VariableValue::String("01xzz-hlw0010001000100010001000".to_owned())
                )
                .0,
            "UNDEF"
        );
        assert_eq!(
            rv32_translator
                .basic_translate(
                    32,
                    &VariableValue::String("010zz-hlw0010001000100010001000".to_owned())
                )
                .0,
            "HIGHIMP"
        );
        assert_eq!(
            rv32_translator
                .basic_translate(
                    32,
                    &VariableValue::String("01011-hlw0010001000100010001000".to_owned())
                )
                .0,
            "DON'T CARE"
        );
    }

    #[test]
    fn mips_from_bigunit() {
        let mips_translator = new_mips_translator();
        assert_eq!(
            mips_translator
                .basic_translate(32, &VariableValue::BigUint(0x3a873u32.into()))
                .0,
            "UNKNOWN INSN (0x0003a873)"
        );
        assert_eq!(
            mips_translator
                .basic_translate(32, &VariableValue::BigUint(0x24210000u32.into()))
                .0,
            "addiu $at, $at, 0"
        );
    }

    #[test]
    fn mips_from_string() {
        let mips_translator = new_mips_translator();
        assert_eq!(
            mips_translator
                .basic_translate(
                    32,
                    &VariableValue::String("10101111110000010000000000000000".to_owned())
                )
                .0,
            "sw $at, 0($fp)"
        );
        assert_eq!(
            mips_translator
                .basic_translate(
                    32,
                    &VariableValue::String("01xzz-hlw0010001000100010001000".to_owned())
                )
                .0,
            "UNDEF"
        );
        assert_eq!(
            mips_translator
                .basic_translate(
                    32,
                    &VariableValue::String("010zz-hlw0010001000100010001000".to_owned())
                )
                .0,
            "HIGHIMP"
        );
        assert_eq!(
            mips_translator
                .basic_translate(
                    32,
                    &VariableValue::String("01011-hlw0010001000100010001000".to_owned())
                )
                .0,
            "DON'T CARE"
        );
    }
}
