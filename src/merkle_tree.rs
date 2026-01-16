//! Merkle tree implementation using Poseidon hashing

use crate::constants::MERKLE_TREE_DEPTH;
use crate::error::{PrivacyCashError, Result};
use crate::keypair::ZkKeypair;
use num_bigint::BigUint;

/// Default zero element for empty leaves
pub const DEFAULT_ZERO: &str = "0";

/// Merkle tree with Poseidon hashing
pub struct MerkleTree {
    /// Number of levels in the tree
    pub levels: usize,

    /// Maximum number of leaves (2^levels)
    pub capacity: usize,

    /// Zero element for empty leaves
    pub zero_element: String,

    /// Precomputed zero values for each level
    zeros: Vec<String>,

    /// Tree layers (layers[0] = leaves)
    layers: Vec<Vec<String>>,
}

impl MerkleTree {
    /// Create a new Merkle tree
    pub fn new(levels: usize) -> Result<Self> {
        Self::with_elements(levels, vec![], DEFAULT_ZERO)
    }

    /// Create a new Merkle tree with initial elements
    pub fn with_elements(levels: usize, elements: Vec<String>, zero_element: &str) -> Result<Self> {
        let capacity = 1usize << levels;

        if elements.len() > capacity {
            return Err(PrivacyCashError::MerkleProofError("Tree is full".to_string()));
        }

        // Initialize zeros for each level
        let mut zeros = Vec::with_capacity(levels + 1);
        zeros.push(zero_element.to_string());

        for i in 1..=levels {
            let prev = &zeros[i - 1];
            let hash = ZkKeypair::poseidon_hash_strings(&[prev, prev])?;
            zeros.push(hash);
        }

        // Initialize layers
        let mut layers: Vec<Vec<String>> = Vec::with_capacity(levels + 1);
        layers.push(elements);

        for _ in 1..=levels {
            layers.push(Vec::new());
        }

        let mut tree = Self {
            levels,
            capacity,
            zero_element: zero_element.to_string(),
            zeros,
            layers,
        };

        tree.rebuild()?;

        Ok(tree)
    }

    /// Rebuild all layers from leaves
    fn rebuild(&mut self) -> Result<()> {
        for level in 1..=self.levels {
            // Clone the previous layer to avoid borrowing issues
            let prev_layer: Vec<String> = self.layers[level - 1].clone();
            let zero_element = self.zeros[level - 1].clone();
            
            self.layers[level].clear();

            let num_pairs = (prev_layer.len() + 1) / 2;

            for i in 0..num_pairs {
                let left = &prev_layer[i * 2];
                let right = if i * 2 + 1 < prev_layer.len() {
                    &prev_layer[i * 2 + 1]
                } else {
                    &zero_element
                };

                let hash = ZkKeypair::poseidon_hash_strings(&[left, right])?;
                self.layers[level].push(hash);
            }
        }

        Ok(())
    }

    /// Get the tree root
    pub fn root(&self) -> String {
        if self.layers[self.levels].is_empty() {
            self.zeros[self.levels].clone()
        } else {
            self.layers[self.levels][0].clone()
        }
    }

    /// Insert a new element into the tree
    pub fn insert(&mut self, element: String) -> Result<()> {
        if self.layers[0].len() >= self.capacity {
            return Err(PrivacyCashError::MerkleProofError("Tree is full".to_string()));
        }

        let index = self.layers[0].len();
        self.update(index, element)
    }

    /// Update an element at a specific index
    pub fn update(&mut self, mut index: usize, element: String) -> Result<()> {
        if index >= self.capacity {
            return Err(PrivacyCashError::MerkleProofError(format!(
                "Index {} out of bounds",
                index
            )));
        }

        // Extend leaves if necessary
        while self.layers[0].len() <= index {
            self.layers[0].push(self.zero_element.clone());
        }

        self.layers[0][index] = element;

        // Update path to root
        for level in 1..=self.levels {
            index >>= 1;

            let prev_layer = &self.layers[level - 1];
            let left_idx = index * 2;
            let right_idx = index * 2 + 1;

            let left = if left_idx < prev_layer.len() {
                &prev_layer[left_idx]
            } else {
                &self.zeros[level - 1]
            };

            let right = if right_idx < prev_layer.len() {
                &prev_layer[right_idx]
            } else {
                &self.zeros[level - 1]
            };

            let hash = ZkKeypair::poseidon_hash_strings(&[left, right])?;

            // Extend current layer if necessary
            while self.layers[level].len() <= index {
                self.layers[level].push(self.zeros[level].clone());
            }

            self.layers[level][index] = hash;
        }

        Ok(())
    }

    /// Bulk insert multiple elements
    pub fn bulk_insert(&mut self, elements: Vec<String>) -> Result<()> {
        if self.layers[0].len() + elements.len() > self.capacity {
            return Err(PrivacyCashError::MerkleProofError("Tree is full".to_string()));
        }

        self.layers[0].extend(elements);
        self.rebuild()
    }

    /// Get Merkle path for a leaf at given index
    pub fn path(&self, index: usize) -> Result<MerklePath> {
        if index >= self.layers[0].len() {
            return Err(PrivacyCashError::MerkleProofError(format!(
                "Index {} out of bounds",
                index
            )));
        }

        let mut path_elements = Vec::with_capacity(self.levels);
        let mut path_indices = Vec::with_capacity(self.levels);
        let mut current_index = index;

        for level in 0..self.levels {
            path_indices.push(current_index % 2);

            let sibling_index = current_index ^ 1;
            let sibling = if sibling_index < self.layers[level].len() {
                self.layers[level][sibling_index].clone()
            } else {
                self.zeros[level].clone()
            };

            path_elements.push(sibling);
            current_index >>= 1;
        }

        Ok(MerklePath {
            path_elements,
            path_indices,
        })
    }

    /// Find index of an element
    pub fn index_of(&self, element: &str) -> Option<usize> {
        self.layers[0].iter().position(|e| e == element)
    }

    /// Get all leaf elements
    pub fn elements(&self) -> Vec<String> {
        self.layers[0].clone()
    }

    /// Get the next available index
    pub fn next_index(&self) -> usize {
        self.layers[0].len()
    }

    /// Get a zero-filled path for dummy UTXOs
    pub fn zero_path() -> MerklePath {
        MerklePath {
            path_elements: vec!["0".to_string(); MERKLE_TREE_DEPTH],
            path_indices: vec![0; MERKLE_TREE_DEPTH],
        }
    }
}

/// Merkle path proof
#[derive(Debug, Clone)]
pub struct MerklePath {
    /// Sibling elements at each level
    pub path_elements: Vec<String>,

    /// Direction indicators (0 = left, 1 = right)
    pub path_indices: Vec<usize>,
}

impl MerklePath {
    /// Verify the path leads to the expected root
    pub fn verify(&self, leaf: &str, expected_root: &str) -> Result<bool> {
        let mut current = leaf.to_string();

        for (element, &index) in self.path_elements.iter().zip(self.path_indices.iter()) {
            let (left, right) = if index == 0 {
                (&current, element)
            } else {
                (element, &current)
            };

            current = ZkKeypair::poseidon_hash_strings(&[left, right])?;
        }

        Ok(current == expected_root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_tree() {
        let tree = MerkleTree::new(4).unwrap();
        let root = tree.root();
        assert!(!root.is_empty());
    }

    #[test]
    fn test_insert_and_path() {
        let mut tree = MerkleTree::new(4).unwrap();

        tree.insert("123".to_string()).unwrap();
        tree.insert("456".to_string()).unwrap();

        let path = tree.path(0).unwrap();
        assert_eq!(path.path_elements.len(), 4);
        assert_eq!(path.path_indices.len(), 4);

        let verified = path.verify("123", &tree.root()).unwrap();
        assert!(verified);
    }

    #[test]
    fn test_tree_capacity() {
        let mut tree = MerkleTree::new(2).unwrap(); // capacity = 4

        tree.insert("1".to_string()).unwrap();
        tree.insert("2".to_string()).unwrap();
        tree.insert("3".to_string()).unwrap();
        tree.insert("4".to_string()).unwrap();

        // Should fail on 5th insert
        let result = tree.insert("5".to_string());
        assert!(result.is_err());
    }
}
