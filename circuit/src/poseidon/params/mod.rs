pub mod bn254_10x5;
pub mod bn254_5x5;

use halo2wrong::halo2::{arithmetic::FieldExt, plonk::Expression};

pub trait RoundParams<F: FieldExt, const WIDTH: usize>: Sbox {
	/// Returns integer 8 as usize.
	fn full_rounds() -> usize;
	/// Returns integer 60 as usize.
	fn partial_rounds() -> usize;

	/// Returns total count size.
	fn round_constants_count() -> usize {
		let partial_rounds = Self::partial_rounds();
		let full_rounds = Self::full_rounds();
		(partial_rounds + full_rounds) * WIDTH
	}

	/// Returns enough length vector to track counting.
	fn round_constants() -> Vec<F> {
		let round_constants_raw = Self::round_constants_raw();
		let round_constants: Vec<F> = round_constants_raw.iter().map(|x| hex_to_field(x)).collect();
		assert_eq!(round_constants.len(), Self::round_constants_count());
		round_constants
	}

	/// Returns relevant constants for the given round.
	fn load_round_constants(round: usize, round_consts: &[F]) -> [F; WIDTH] {
		let mut result = [F::zero(); WIDTH];
		for i in 0..WIDTH {
			result[i] = round_consts[round * WIDTH + i];
		}
		result
	}

	/// Returns fixed string array values as field elements.
	fn mds() -> [[F; WIDTH]; WIDTH] {
		let mds_raw = Self::mds_raw();
		mds_raw.map(|row| row.map(|item| hex_to_field(item)))
	}

	/// Returns fixed string vector.
	fn round_constants_raw() -> Vec<&'static str>;
	/// Returns fixed string array.
	fn mds_raw() -> [[&'static str; WIDTH]; WIDTH];
}

pub trait Sbox {
	/// Returns exp^5.
	fn sbox_expr<F: FieldExt>(exp: Expression<F>) -> Expression<F>;
	/// Returns f^5.
	fn sbox_f<F: FieldExt>(f: F) -> F;
}

/// Returns congruent Field element for the given hex string.
pub fn hex_to_field<F: FieldExt>(s: &str) -> F {
	let s = &s[2..];
	let mut bytes = hex::decode(s).expect("Invalid params");
	bytes.reverse();
	let mut bytes_wide: [u8; 64] = [0; 64];
	bytes_wide[..bytes.len()].copy_from_slice(&bytes[..]);
	F::from_bytes_wide(&bytes_wide)
}
