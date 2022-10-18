use std::vec;

use super::Signature;
use crate::{constants::*, epoch::Epoch, error::EigenError, utils::to_wide_bytes};
use bs58::decode::Error as Bs58Error;
use eigen_trust_circuit::{
	halo2wrong::{
		curves::{
			bn256::{Bn256, Fr as Bn256Scalar, G1Affine},
			FieldExt,
		},
		halo2::{
			plonk::{ProvingKey, VerifyingKey},
			poly::kzg::commitment::ParamsKZG,
		},
	},
	params::poseidon_bn254_5x5::Params,
	poseidon::native::Poseidon,
	utils::{prove, verify},
	EigenTrustCircuit,
};
use libp2p::core::identity::Keypair as IdentityKeypair;
use rand::thread_rng;

pub type Posedion5x5 = Poseidon<Bn256Scalar, 5, Params>;
pub type ETCircuit = EigenTrustCircuit<Bn256Scalar, MAX_NEIGHBORS, NUM_BOOTSTRAP_PEERS, Params>;
pub const SCALE: f64 = 100000000.;

#[derive(Clone, Debug, PartialEq)]
pub struct IVP {
	pub(crate) epoch: Epoch,
	pub(crate) iter: u32,
	pub(crate) op: f64,
	pub(crate) proof_bytes: Vec<u8>,
	pub(crate) m_hash: [u8; 32],
}

impl IVP {
	pub fn new(epoch: Epoch, iter: u32, op: f64, proof_bytes: Vec<u8>) -> Self {
		Self { epoch, iter, op, proof_bytes, m_hash: [0; 32] }
	}

	/// Creates a new IVP.
	pub fn generate(
		sig: &Signature, pk_v: Bn256Scalar, epoch: Epoch, k: u32, op_ji: [f64; MAX_NEIGHBORS],
		c_v: f64, params: &ParamsKZG<Bn256>, pk: &ProvingKey<G1Affine>,
	) -> Result<Self, EigenError> {
		let mut rng = thread_rng();

		let input = [
			Bn256Scalar::zero(),
			Bn256Scalar::zero(),
			Bn256Scalar::zero(),
			Bn256Scalar::zero(),
			sig.sk,
		];
		let pos = Posedion5x5::new(input);
		let pk_p = pos.permute()[0];

		let bootstrap_pubkeys = BOOTSTRAP_PEERS
			.try_map(|key| {
				let bytes = &bs58::decode(key).into_vec()?;
				Ok(Bn256Scalar::from_bytes_wide(&to_wide_bytes(bytes)))
			})
			.map_err(|_: Bs58Error| EigenError::InvalidBootstrapPubkey)?;
		let bootstrap_score_scaled = (BOOTSTRAP_SCORE * SCALE).round() as u128;
		// Turn into scaled values and round the to avoid rounding errors.
		let op_ji_scaled = op_ji.map(|op| (op * SCALE).round() as u128);
		let c_v_scaled = (c_v * SCALE).round() as u128;

		let t_i_scaled = op_ji_scaled.iter().sum();

		let is_bootstrap = bootstrap_pubkeys.contains(&pk_p);
		let is_first_iter = k == 0;
		let t_i_final =
			if is_bootstrap && is_first_iter { bootstrap_score_scaled } else { t_i_scaled };

		let op_v_scaled = t_i_final * c_v_scaled;
		let op_v_unscaled = (op_v_scaled as f64) / (SCALE * SCALE);
		// Converting into field
		let op_ji_f = op_ji_scaled.map(Bn256Scalar::from_u128);
		let c_v_f = Bn256Scalar::from_u128(c_v_scaled);
		let op_v_f = Bn256Scalar::from_u128(op_v_scaled);
		let epoch_f = Bn256Scalar::from(epoch.0);
		let iter_f = Bn256Scalar::from_u128(u128::from(k));
		let bootstrap_score_f = Bn256Scalar::from_u128(bootstrap_score_scaled);

		let m_hash_input = [epoch_f, iter_f, op_v_f, pk_v, pk_p];
		let pos = Posedion5x5::new(m_hash_input);
		let m_hash = pos.permute()[0];

		let circuit = ETCircuit::new(
			pk_v, epoch_f, iter_f, sig.sk, op_ji_f, c_v_f, bootstrap_pubkeys, bootstrap_score_f,
		);

		let pub_ins = vec![m_hash];

		let proof_bytes = prove(params, circuit, &[&pub_ins], pk, &mut rng)
			.map_err(|_| EigenError::ProvingError)?;

		// Sanity check
		let proof_res = verify(params, &[&pub_ins], &proof_bytes, pk.get_vk()).map_err(|e| {
			println!("{}", e);
			EigenError::VerificationError
		})?;
		assert!(proof_res);

		Ok(Self { epoch, iter: k, op: op_v_unscaled, proof_bytes, m_hash: m_hash.to_bytes() })
	}

	/// Verifies the proof.
	pub fn verify(
		&self, sk_p: Bn256Scalar, pubkey_p: Bn256Scalar, params: &ParamsKZG<Bn256>,
		vk: &VerifyingKey<G1Affine>,
	) -> Result<bool, EigenError> {
		let input = [
			Bn256Scalar::zero(),
			Bn256Scalar::zero(),
			Bn256Scalar::zero(),
			Bn256Scalar::zero(),
			sk_p,
		];
		let pos = Posedion5x5::new(input);
		let pk_v = pos.permute()[0];

		let op_v_scaled = (self.op * SCALE * SCALE).round() as u128;
		let epoch_f = Bn256Scalar::from_u128(u128::from(self.epoch.0));
		let iter_f = Bn256Scalar::from_u128(u128::from(self.iter));
		let op_v_f = Bn256Scalar::from_u128(op_v_scaled);

		let m_hash_input = [epoch_f, iter_f, op_v_f, pk_v, pubkey_p];
		let pos = Posedion5x5::new(m_hash_input);
		let m_hash = pos.permute()[0];
		let m_hash_passed = Bn256Scalar::from_bytes(&self.m_hash).unwrap();

		let final_hash = if op_v_f == Bn256Scalar::zero() { m_hash_passed } else { m_hash };
		let pub_ins = vec![final_hash];

		let proof_res = verify(params, &[&pub_ins], &self.proof_bytes, vk).map_err(|e| {
			println!("{}", e);
			EigenError::VerificationError
		})?;

		Ok(proof_res)
	}
}
