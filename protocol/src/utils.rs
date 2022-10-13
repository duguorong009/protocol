use crate::{manager::ivp::Posedion5x5, error::EigenError};
use eigen_trust_circuit::halo2wrong::{
	curves::{bn256::Fr as Bn256Scalar, secp256k1::Fq as Secp256k1Scalar, FieldExt},
	utils::decompose, halo2::arithmetic::Field,
};
use futures::{
	stream::{self, BoxStream, Fuse},
	StreamExt,
};
use libp2p::core::identity::{
	secp256k1::{Keypair as Secp256k1Keypair, SecretKey},
	Keypair as IdentityKeypair,
};
use rand::RngCore;
use tokio::time::{self, Duration, Instant};

pub fn generate_keypair_from_sk(sk: Bn256Scalar) -> Result<Bn256Scalar, EigenError> {
	let input = [
		Bn256Scalar::zero(),
		Bn256Scalar::zero(),
		Bn256Scalar::zero(),
		Bn256Scalar::zero(),
		sk,
	];
	let pos = Posedion5x5::new(input);
	let out = pos.permute()[0];

	Ok(out)
}

/// Hash the secret key limbs with Poseidon.
pub fn generate_keypair<R: RngCore>(rng: &mut R) -> Result<Bn256Scalar, EigenError> {
	let sk = Bn256Scalar::random(rng);

	generate_keypair_from_sk(sk)
}

/// Write an array of 32 elements into an array of 64 elements.
pub fn to_wide(p: [u8; 32]) -> [u8; 64] {
	let mut res = [0u8; 64];
	res[..32].copy_from_slice(&p[..]);
	res
}

/// Write a byte array into an array of 64 elements.
pub fn to_wide_bytes(p: &[u8]) -> [u8; 64] {
	let mut res = [0u8; 64];
	res[..p.len()].copy_from_slice(p);
	res
}

/// Schedule `num` intervals with a duration of `interval` that starts at
/// `start`.
pub fn create_iter<'a>(start: Instant, interval: Duration, num: usize) -> Fuse<BoxStream<'a, u32>> {
	let mut inner_interval = time::interval_at(start, interval);
	inner_interval.set_missed_tick_behavior(time::MissedTickBehavior::Skip);
	stream::unfold((inner_interval, 0), |(mut interval, count)| async move {
		interval.tick().await;
		Some((count, (interval, count + 1)))
	})
	.take(num)
	.boxed()
	.fuse()
}
