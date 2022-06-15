//! The module for the peer related functionalities, like:
//! - Adding/removing neighbors
//! - Calculating the global trust score
//! - Calculating local scores toward neighbors for a given epoch
//! - Keeping track of neighbors scores towards us

use crate::{epoch::Epoch, EigenError};
use libp2p::PeerId;
use std::collections::HashMap;

/// The struct for opinions between peers at the specific epoch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opinion {
	k: Epoch,
	local_trust_score: f64,
	global_trust_score: f64,
	product: f64,
}

impl Opinion {
	/// Creates a new opinion.
	pub fn new(k: Epoch, local_trust_score: f64, global_trust_score: f64, product: f64) -> Self {
		Self {
			k,
			local_trust_score,
			global_trust_score,
			product,
		}
	}

	/// Creates an empty opinion, in a case when we don't have any opinion about
	/// a peer, or the neighbor doesn't have any opinion about us.
	pub fn empty(k: Epoch) -> Self {
		Self::new(k, 0.0, 0.0, 0.0)
	}

	/// Returns the epoch of the opinion.
	pub fn get_epoch(&self) -> Epoch {
		self.k
	}

	/// Returns the local trust score of the opinion.
	pub fn get_local_trust_score(&self) -> f64 {
		self.local_trust_score
	}

	/// Returns the global trust score of the opinion.
	pub fn get_global_trust_score(&self) -> f64 {
		self.global_trust_score
	}

	/// Returns the product of global and local trust score of the opinion.
	pub fn get_product(&self) -> f64 {
		self.product
	}
}

/// The peer struct.
pub struct Peer {
	neighbor_scores: HashMap<PeerId, u32>,
	neighbors: Vec<Option<PeerId>>,
	cached_neighbor_opinion: HashMap<(PeerId, Epoch), Opinion>,
	cached_local_opinion: HashMap<(PeerId, Epoch), Opinion>,
	pre_trust_score: f64,
	pre_trust_weight: f64,
}

impl Peer {
	/// Creates a new peer.
	pub fn new(num_neighbors: usize, pre_trust_score: f64, pre_trust_weight: f64) -> Self {
		// Our neighbors array is fixed size.
		// TODO: Consider using ArrayVec instead.
		let mut neighbors = Vec::with_capacity(num_neighbors);
		for _ in 0..num_neighbors {
			neighbors.push(None);
		}
		Peer {
			neighbors,
			neighbor_scores: HashMap::new(),
			cached_neighbor_opinion: HashMap::new(),
			cached_local_opinion: HashMap::new(),
			pre_trust_score,
			pre_trust_weight,
		}
	}

	/// Adds a neighbor in the first available spot.
	pub fn add_neighbor(&mut self, peer_id: PeerId) -> Result<(), EigenError> {
		if self.neighbors.contains(&Some(peer_id)) {
			return Ok(());
		}
		let index = self
			.neighbors
			.iter()
			.position(|&x| x.is_none())
			.ok_or(EigenError::MaxNeighboursReached)?;
		self.neighbors[index] = Some(peer_id);
		Ok(())
	}

	/// Removes a neighbor, if found.
	pub fn remove_neighbor(&mut self, peer_id: PeerId) {
		let index_res = self.neighbors.iter().position(|&x| x == Some(peer_id));
		if let Some(index) = index_res {
			self.neighbors[index] = None;
		}
	}

	/// Returns the neighbors of the peer.
	pub fn neighbors(&self) -> Vec<PeerId> {
		self.neighbors.iter().filter_map(|&x| x).collect()
	}

	/// Set the local score towards a neighbor.
	pub fn set_score(&mut self, peer_id: PeerId, score: u32) {
		self.neighbor_scores.insert(peer_id, score);
	}

	/// Calculate the global trust score of the peer in the specified epoch.
	/// We do this by taking the sum of neighbor's opinions and weighting it by
	/// the pre trust weight. Then we are adding it with the weighted pre-trust
	/// score.
	pub fn calculate_global_trust_score(&self, epoch: Epoch) -> f64 {
		let mut global_score = 0.;

		for peer_id in self.neighbors() {
			let opinion = self.get_neighbor_opinion(&(peer_id, epoch));
			global_score += opinion.get_product();
		}
		// We are adding the weighted pre trust score to the weighted score.
		global_score = (1. - self.pre_trust_weight) * global_score
			+ self.pre_trust_weight * self.pre_trust_score;

		global_score
	}

	/// Calculate the local trust score toward all neighbors in the specified
	/// epoch.
	pub fn calculate_local_opinions(&mut self, k: Epoch) {
		let global_score = self.calculate_global_trust_score(k);

		let mut opinions = Vec::new();
		for peer_id in self.neighbors() {
			let score = self.neighbor_scores.get(&peer_id).unwrap_or(&0);
			let normalized_score = self.get_normalized_score(*score);
			let product = global_score * normalized_score;
			let opinion = Opinion::new(k.next(), normalized_score, global_score, product);

			opinions.push((peer_id, opinion));
		}

		for (peer_id, opinion) in opinions {
			self.cache_local_opinion((peer_id, opinion.get_epoch()), opinion);
		}
	}

	/// Returns sum of local scores.
	pub fn get_sum_of_scores(&self) -> u32 {
		let mut sum = 0;
		for peer_id in self.neighbors() {
			let score = self.neighbor_scores.get(&peer_id).unwrap_or(&0);
			sum += score;
		}
		sum
	}

	/// Returns the normalized score.
	pub fn get_normalized_score(&self, score: u32) -> f64 {
		let sum = self.get_sum_of_scores();
		let f_raw_score = f64::from(score);
		let f_sum = f64::from(sum);
		f_raw_score / f_sum
	}

	/// Returns the local score towards a neighbor in a specified epoch.
	pub fn get_local_opinion(&self, key: &(PeerId, Epoch)) -> Opinion {
		*self
			.cached_local_opinion
			.get(key)
			.unwrap_or(&Opinion::empty(key.1))
	}

	/// Caches the local opinion towards a peer in a specified epoch.
	pub fn cache_local_opinion(&mut self, key: (PeerId, Epoch), opinion: Opinion) {
		self.cached_local_opinion.insert(key, opinion);
	}

	/// Returns the neighbor's opinion towards us in a specified epoch.
	pub fn get_neighbor_opinion(&self, key: &(PeerId, Epoch)) -> Opinion {
		*self
			.cached_neighbor_opinion
			.get(key)
			.unwrap_or(&Opinion::empty(key.1))
	}

	/// Caches the neighbor opinion towards us in specified epoch.
	pub fn cache_neighbor_opinion(&mut self, key: (PeerId, Epoch), opinion: Opinion) {
		self.cached_neighbor_opinion.insert(key, opinion);
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const NUM_CONNECTIONS: usize = 256;

	#[test]
	fn should_create_opinion() {
		let opinion = Opinion::new(Epoch(0), 0.5, 0.5, 0.5);
		assert_eq!(opinion.get_epoch(), Epoch(0));
		assert_eq!(opinion.get_global_trust_score(), 0.5);
		assert_eq!(opinion.get_local_trust_score(), 0.5);
		assert_eq!(opinion.get_product(), 0.5);
	}

	#[test]
	fn should_create_peer() {
		let peer = Peer::new(NUM_CONNECTIONS, 0.5, 0.5);
		assert_eq!(peer.pre_trust_score, 0.5);
		assert_eq!(peer.pre_trust_weight, 0.5);
		assert_eq!(peer.get_sum_of_scores(), 0);
	}

	#[test]
	fn should_cache_local_and_global_opinion() {
		let pre_trust_score = 0.5;
		let pre_trust_weight = 0.5;
		let mut peer = Peer::new(NUM_CONNECTIONS, pre_trust_score, pre_trust_weight);

		let epoch = Epoch(0);
		let neighbor_id = PeerId::random();
		let opinion = Opinion::new(epoch, 0.5, 0.5, 0.25);
		peer.cache_local_opinion((neighbor_id, epoch), opinion);
		peer.cache_neighbor_opinion((neighbor_id, epoch), opinion);

		assert_eq!(peer.get_local_opinion(&(neighbor_id, epoch)), opinion);
		assert_eq!(peer.get_neighbor_opinion(&(neighbor_id, epoch)), opinion);
	}

	#[test]
	fn should_add_and_remove_neghbours() {
		let mut peer = Peer::new(NUM_CONNECTIONS, 0.5, 0.5);
		let neighbor_id = PeerId::random();

		peer.add_neighbor(neighbor_id).unwrap();
		let num_neighbors = peer.neighbors().len();
		assert_eq!(num_neighbors, 1);

		peer.remove_neighbor(neighbor_id);
		let num_neighbors = peer.neighbors().len();
		assert_eq!(num_neighbors, 0);
	}

	#[test]
	fn should_add_neighbors_and_calculate_global_score() {
		let pre_trust_score = 0.5;
		let pre_trust_weight = 0.5;
		let mut peer = Peer::new(NUM_CONNECTIONS, pre_trust_score, pre_trust_weight);

		let epoch = Epoch(0);
		for _ in 0..256 {
			let peer_id = PeerId::random();
			peer.add_neighbor(peer_id).unwrap();
			peer.set_score(peer_id, 5);
			let opinion = Opinion::new(epoch, 0.1, 0.1, 0.01);
			peer.cache_neighbor_opinion((peer_id, epoch), opinion);
		}

		let global_score = peer.calculate_global_trust_score(epoch);

		let mut true_global_score = 0.0;
		for _ in 0..256 {
			true_global_score += 0.01;
		}
		let boostrap_score =
			(1. - pre_trust_weight) * true_global_score + pre_trust_weight * pre_trust_score;

		assert_eq!(boostrap_score, global_score);
	}

	#[test]
	fn should_add_neighbors_and_calculate_local_scores() {
		let pre_trust_score = 0.5;
		let pre_trust_weight = 0.5;
		let mut peer = Peer::new(NUM_CONNECTIONS, pre_trust_score, pre_trust_weight);

		let epoch = Epoch(0);
		for _ in 0..256 {
			let peer_id = PeerId::random();
			peer.add_neighbor(peer_id).unwrap();
			peer.set_score(peer_id, 5);
			let opinion = Opinion::new(epoch, 0.1, 0.1, 0.01);
			peer.cache_neighbor_opinion((peer_id, epoch), opinion);
		}

		let global_score = peer.calculate_global_trust_score(epoch);

		peer.calculate_local_opinions(epoch);

		for peer_id in peer.neighbors() {
			let opinion = peer.get_local_opinion(&(peer_id, epoch.next()));
			let score = peer.neighbor_scores.get(&peer_id).unwrap_or(&0);
			let normalized_score = peer.get_normalized_score(*score);
			let local_score = normalized_score * global_score;
			assert_eq!(opinion.get_product(), local_score);
		}
	}
}