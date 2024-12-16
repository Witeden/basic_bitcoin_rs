use crate::blockchain::hash_to_hex_string;
use hex;
use secp256k1::{ecdsa::Signature, Message, PublicKey, Secp256k1};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

/// Transactions have a list of input, referencing previous transactions, two outputs (one for the payment, one for the change), and a signature, produced by the author of the transaction.
/// Transactions are verified by checking that:
/// - all inputs come from the same user (referenced by its public key),
/// - the signature of the transaction is correct
/// - the sum of the inputs is greater than the sum of the outputs

#[derive(Debug, Clone, PartialEq)]
pub struct Transaction {
    pub inputs: Vec<Input>,
    pub payment: Output,
    pub change: Output,
    pub sig: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    pub origin_tx: String,
    pub value: u64,
    pub pubkey: String,
    pub payment_type: bool,
}

impl Input {
    pub fn new(origin_tx: &str, value: u64, pubkey: &str, payment_type: bool) -> Self {
        Input {
            origin_tx: origin_tx.to_string(),
            value,
            pubkey: pubkey.to_string(),
            payment_type,
        }
    }
    pub fn is_payment(&self) -> bool {
        self.payment_type
    }
    pub fn from_output_tx(tx: &Transaction, payment_type: bool) -> Self {
        let value = if payment_type {
            tx.payment.value
        } else {
            tx.change.value
        };
        let pubkey = if payment_type {
            tx.payment.pubkey.clone()
        } else {
            tx.change.pubkey.clone()
        };
        Input {
            origin_tx: tx.hash(),
            value,
            pubkey,
            payment_type,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub pubkey: String,
    pub value: u64,
}

impl Output {
    pub fn new(pubkey: &str, value: u64) -> Self {
        Output {
            pubkey: pubkey.to_string(),
            value,
        }
    }
}

impl Transaction {
    pub fn new(inputs: Vec<Input>, payment: Output, change: Output, sig: &str) -> Self {
        Transaction {
            inputs,
            payment,
            change,
            sig: sig.to_string(),
        }
    }

    pub fn input_ids(&self) -> Vec<String> {
        self.inputs
            .iter()
            .map(|input| input.origin_tx.to_string())
            .collect()
    }
    pub fn hash(&self) -> String {
        let mut to_hash: String = self
            .inputs
            .iter()
            .map(|input| input.origin_tx.clone())
            .collect::<Vec<String>>()
            .join("_");

        to_hash = [to_hash, self.payment.pubkey.clone()].join("_");
        let mut hasher = Sha256::new();
        hasher.update(to_hash.as_bytes());
        hash_to_hex_string(hasher.finalize().as_slice())
    }

    pub fn input_value(&self) -> u64 {
        if self.sig != "Authority transaction" {
            self.inputs.iter().map(|input| input.value).sum()
        } else {
            self.output_value()
        }
    }

    pub fn output_value(&self) -> u64 {
        self.payment.value + self.change.value
    }

    pub fn has_valid_balance(&self) -> bool {
        self.input_value() >= self.output_value()
    }

    pub fn has_valid_signature(&self) -> bool {
        if let Some(input) = self.inputs.first() {
            let secp = Secp256k1::verification_only();
            let hash_bytes: [u8; 32] = hex::decode(self.hash())
                .expect("Could not decode string as hex chain")
                .try_into()
                .unwrap_or_else(|_| unreachable!("SHA256 has a 32 bytes output"));

            let to_check = Message::from_digest(hash_bytes);

            let sig: Signature = Signature::from_der(&hex::decode(&self.sig).unwrap_or_default())
                .expect("Transaction has invalid signature.");
            let pubkey = PublicKey::from_slice(&hex::decode(&input.pubkey).unwrap_or_default())
                .expect("Public key must be 33 or 65 bytes");
            // we check that the signature is valid

            secp.verify_ecdsa(&to_check, &sig, &pubkey).is_ok()
        } else {
            self.sig == "Authority transaction".to_string()
        }
    }

    pub fn has_valid_inputs(&self) -> bool {
        if let Some(input) = self.inputs.first() {
            self.inputs
                .iter()
                .filter(|other_input| other_input.pubkey != input.pubkey)
                .count()
                == 0
        } else {
            self.sig == "Authority transaction".to_string()
        }
    }

    pub fn input_matches_output(&self, input: &Input) -> bool {
        if input.payment_type {
            self.payment.pubkey == input.pubkey
        } else {
            self.change.pubkey == input.pubkey
        }
    }
}

fn hash_nodes(left_node: String, right_node: String) -> String {
    let to_hash = [left_node, right_node].join("");
    let mut hasher = Sha256::new();
    hasher.update(to_hash.as_bytes());
    hash_to_hex_string(hasher.finalize().as_slice())
}

fn merkle_root(hash_list: Vec<String>) -> String {
    if let Some(last) = hash_list.last() {
        if hash_list.len() == 1 {
            // We are at the root of the tree
            last.clone()
        } else {
            let mut next_level: Vec<String> = Vec::with_capacity((hash_list.len() + 1) / 2);
            if hash_list.len() % 2 == 1 {
                next_level.push(last.clone());
            }
            hash_list
                .windows(2)
                .enumerate()
                .filter(|(i, _)| i % 2 == 0)
                .for_each(|(_, hash_pair)| {
                    let (left_hash, right_hash) = (hash_pair[0].clone(), hash_pair[1].clone());
                    next_level.push(hash_nodes(left_hash, right_hash));
                });
            merkle_root(next_level)
        }
    } else {
        // return SHA256 hash of an empty string
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_owned()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TxBook {
    /// Holds the set of transactions, and memoizes unspent transactions (utxos).
    /// Can be used either for the generator to hold the set of current transactions,
    /// or for a block to hold the set of registered transactions.
    pub book: HashMap<String, Transaction>,
    pub utxos: HashSet<String>,
}

impl TxBook {
    pub fn new(book: HashMap<String, Transaction>, utxos: HashSet<String>) -> Self {
        Self { book, utxos }
    }
    pub fn txs(&self) -> std::collections::hash_map::Iter<'_, std::string::String, Transaction> {
        self.book.iter()
    }
    pub fn get_tx(&self, txid: &str) -> Option<&Transaction> {
        self.book.get(txid)
    }

    pub fn add_utxo(&mut self, tx_id: &str) {
        if self.book.contains_key(tx_id) {
            self.utxos.insert(tx_id.to_string());
        }
    }

    pub fn add_tx(&mut self, tx: Transaction) {
        tx.inputs.iter().for_each(|input| {
            self.utxos.remove(&input.origin_tx);
        });
        self.book.insert(tx.hash(), tx);
    }

    pub fn find_utxos(&self) -> HashSet<String> {
        let mut utxos: HashSet<String> = self.book.keys().cloned().collect();
        for tx in self.book.values() {
            tx.input_ids().iter().for_each(|id| {
                utxos.remove(id);
            });
        }
        utxos
    }

    pub fn contains_tx(&self, tx_id: &str) -> bool {
        self.book.contains_key(tx_id)
    }

    pub fn n_tx(&self) -> usize {
        self.book.len()
    }

    pub fn clear_tx(&mut self, tx_id: &str) {
        let mut to_clear: HashSet<String> = HashSet::new();
        to_clear.insert(tx_id.to_string());
        while let Some(clear_tx_id) = to_clear.iter().next().cloned() {
            to_clear.remove(&clear_tx_id);
            self.utxos.remove(&clear_tx_id);

            if let Some(tx) = self.get_tx(&clear_tx_id) {
                self.book.iter().for_each(|(_, output_tx)| {
                    if output_tx
                        .input_ids()
                        .iter()
                        .any(|id| id.to_string() == clear_tx_id)
                    {
                        to_clear.insert(tx.hash());
                    }
                })
            }
            self.book.remove(&clear_tx_id);
        }
    }

    pub fn merkle_root(&self) -> String {
        let mut hash_list = self.book.keys().cloned().collect::<Vec<String>>();
        // We sort to make sure that the order on the keys is always the same
        hash_list.sort();
        merkle_root(hash_list)
    }

    pub fn has_valid_inputs(&self, tx: &Transaction) -> bool {
        tx.inputs
            .iter()
            .map(|input| {
                if let Some(input_tx) = self.get_tx(&input.origin_tx) {
                    if input.is_payment() {
                        input_tx.payment.pubkey == input.pubkey
                    } else {
                        input_tx.change.pubkey == input.pubkey
                    }
                } else {
                    false
                }
            })
            .all(|res| res)
    }
}

impl Default for TxBook {
    fn default() -> Self {
        TxBook {
            book: HashMap::new(),
            utxos: HashSet::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_small_tree() {
        let mut hash_list = vec![
            "this".to_string(),
            "is".to_string(),
            "a".to_string(),
            "test".to_string(),
        ];
        hash_list.iter_mut().for_each(|word| {
            let mut hasher = Sha256::new();
            hasher.update(word.as_bytes());
            *word = hash_to_hex_string(hasher.finalize().as_slice());
        });
        let expected_root =
            "b1ab80e998ac8414b7c6b39ab0cf84489d1a08532370dc1b77b793bb67a8f3e1".to_string();

        assert_eq!(expected_root, merkle_root(hash_list));
    }
}
