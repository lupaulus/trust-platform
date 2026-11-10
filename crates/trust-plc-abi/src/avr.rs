//! The AVR's missing compiler-rt shim (specification 34 D.6): LLVM's AVR backend lowers a 32-bit
//! multiply to a call of `__mulsi3`, which compiler-builtins provides only for riscv and m68k
//! (spike of 10 November 2026, the portability checklist's checkpoint). Every AVR module links this
//! crate, so the shim lives here.
//!
//! The shim is built from 8x8 partial products: the AVR's `mul`/`muls` expand a u8 or u16
//! multiply inline, so this never recurses into `__mulsi3` itself (a u32 multiply would).

/// 8x8 to 16 bits: the product of two bytes fits in 16 bits, so the wrapping u16 product is exact.
#[cfg(any(target_arch = "avr", test))]
#[inline(always)]
fn mul8x8(a: u8, b: u8) -> u16 {
    (a as u16).wrapping_mul(b as u16)
}

/// 16x16 to 32 bits from 8x8 partial products: only u16 multiplies, adds and shifts.
#[cfg(any(target_arch = "avr", test))]
#[inline(always)]
fn mul16x16(a: u16, b: u16) -> u32 {
    let (a0, a1) = (a as u8, (a >> 8) as u8);
    let (b0, b1) = (b as u8, (b >> 8) as u8);
    let p00 = mul8x8(a0, b0) as u32;
    let p01 = mul8x8(a0, b1) as u32;
    let p10 = mul8x8(a1, b0) as u32;
    let p11 = mul8x8(a1, b1) as u32;
    p00 + ((p01 + p10) << 8) + (p11 << 16)
}

/// The low 32 bits of a 32x32 product, from 16x16 partial products (the high-high product
/// contributes nothing below bit 32).
#[cfg(any(target_arch = "avr", test))]
#[inline(always)]
fn mul32(a: u32, b: u32) -> u32 {
    let (al, ah) = (a as u16, (a >> 16) as u16);
    let (bl, bh) = (b as u16, (b >> 16) as u16);
    mul16x16(al, bl)
        .wrapping_add(mul16x16(al, bh) << 16)
        .wrapping_add(mul16x16(ah, bl) << 16)
}

/// The low 32 bits of a 32x32 product, for a module's own `__mulsi3` shim: the shim must be defined
/// in the module crate itself, because a `no_mangle` symbol in a linked rlib is internalized away
/// by the module's link-time optimization before the backend lowers the multiply to a call of it.
#[cfg(target_arch = "avr")]
#[must_use]
pub fn avr_mulsi3(a: u32, b: u32) -> u32 {
    mul32(a, b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shim must agree with the wrapping u32 product on every value the 8x8/16x16 partials
    /// cover, including the carries between them. Catches a partial product dropped or shifted
    /// wrong (a product that differs from `a.wrapping_mul(b)`) and a shim that would recurse
    /// into itself (a u32 multiply inside, which does not link on AVR).
    #[test]
    fn mul32_agrees_with_the_wrapping_product() {
        let values = [
            0,
            1,
            2,
            255,
            256,
            65_535,
            65_536,
            0x1234_5678,
            0xFFFF_FFFF,
            0x8000_0000,
        ];
        for a in values {
            for b in values {
                assert_eq!(mul32(a, b), a.wrapping_mul(b), "{a:#x} * {b:#x}");
            }
        }
        assert_eq!(mul32(0x1_0000, 0x1_0000), 0);
        assert_eq!(mul16x16(0xFFFF, 0xFFFF), 0xFFFE_0001);
    }
}
