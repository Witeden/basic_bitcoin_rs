use crate::transaction::{Transaction, TxBook};
use hex;
use serde_json;
use sha2::{Digest, Sha256};
use thiserror::Error;
/// The blockchain struct is a simple vector of blocks.
/// A block has a blockheader and a list of transactions.

pub type Timestamp = u32;
pub(crate) const DIFFICULTY_PREFIX: &str = "00";

pub(crate) fn hash_to_hex_string(hash: &[u8]) -> String {
    let mut res: String = String::default();
    for c in hash {
        res.push_str(&format!("{:02x}", c));
    }
    res
}

fn is_chain_valid(chain: &[Block]) -> bool {
    chain
        .windows(2)
        .map(|block_pair| {
            let [first, second] = block_pair else {
                unreachable!()
            };
            first.is_valid_next(second)
        })
        .all(|b| b)
}

pub(crate) fn mine_block(
    id: u64,
    timestamp: i64,
    prev_hash: &str,
    transactions: &TxBook,
    nonce_start: u64,
) -> Block {
    let next_block_header = BlockHeader::new(
        timestamp,
        prev_hash,
        nonce_start,
        transactions.merkle_root(),
    );
    let mut next_block = Block {
        header: next_block_header,
        id,
        transactions: transactions.clone(),
        hash: "".to_string(),
    };
    while !hash_to_hex_string(&next_block.hash()).starts_with(DIFFICULTY_PREFIX) {
        next_block.increment_nonce();
        println!("nonce: {}", next_block.get_nonce());
    }
    next_block
}

#[derive(Debug, Error)]
pub enum BlockchainError {
    #[error("Attempt to access last block, while blockchain is empty.")]
    EmptyChain,
    #[error("Attempt to access last block, while blockchain is empty.")]
    InvalidBlock((Block, Block)),
}

#[derive(Debug, Clone)]
pub struct BlockHeader {
    timestamp: i64,
    prev_hash: String,
    nonce: u64,
    merkle_root: String,
}

impl BlockHeader {
    pub fn new(timestamp: i64, prev_hash: &str, nonce: u64, merkle_root: String) -> Self {
        Self {
            prev_hash: prev_hash.to_string(),
            nonce,
            merkle_root,
            timestamp,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Block {
    header: BlockHeader,
    pub id: u64,
    pub transactions: TxBook,
    hash: String,
}

impl Block {
    pub fn new(
        id: u64,
        timestamp: i64,
        prev_hash: &str,
        nonce: u64,
        transactions: &TxBook,
    ) -> Self {
        let merkle_root = transactions.merkle_root();
        let block_header = BlockHeader::new(timestamp, prev_hash, nonce, merkle_root);
        let mut block = Block {
            header: block_header,
            id,
            transactions: transactions.clone(),
            hash: "".to_string(),
        };
        block.hash = hash_to_hex_string(&block.hash());
        block
    }

    pub fn from_mining(id: u64, timestamp: i64, prev_hash: String, transactions: TxBook) -> Self {
        mine_block(id, timestamp, &prev_hash, &transactions, 0)
    }
    fn increment_nonce(&mut self) {
        self.header.nonce += 1;
    }

    pub fn set_nonce(&mut self, nonce: u64) {
        self.header.nonce = nonce;
    }

    pub fn get_nonce(&self) -> u64 {
        self.header.nonce
    }

    pub fn set_timestamp(&mut self, timestamp: i64) {
        self.header.timestamp = timestamp;
    }

    pub fn set_id(&mut self, id: u64) {
        self.id = id;
    }

    pub fn hash(&self) -> Vec<u8> {
        let data = serde_json::json!({
            "id": self.id,
            "previous_hash": self.header.prev_hash,
            "merkle_root": self.header.merkle_root,
            "timestamp": self.header.timestamp,
            "nonce": self.header.nonce
        });
        let mut hasher = Sha256::new();
        hasher.update(data.to_string().as_bytes());
        hasher.finalize().as_slice().to_owned()
    }
    pub fn is_valid_next(&self, next: &Block) -> bool {
        if self.hash != next.header.prev_hash {
            return false;
        // Need to check that block begins with difficulty prefix
        } else if !(hash_to_hex_string(
            &hex::decode(&next.hash).expect("Could not decode hash from hex."),
        )
        .starts_with(DIFFICULTY_PREFIX))
        {
            return false;
        } else if !next.hash.starts_with(DIFFICULTY_PREFIX) {
            return false;
        } else if next.id != self.id + 1 {
            return false;
        } else if next.hash != hex::encode(next.hash()) {
            return false;
        }

        true
    }
}

#[derive(Debug, Clone)]
pub struct Blockchain {
    pub blocks: Vec<Block>,
}

impl Blockchain {
    pub fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    pub fn last(&self) -> Option<&Block> {
        self.blocks.last()
    }

    pub fn len(&self) -> usize {
        self.blocks.len()
    }

    pub fn try_add_block(&mut self, new_block: Block) -> Result<(), BlockchainError> {
        match self.last() {
            Some(last_block) => {
                if last_block.is_valid_next(&new_block) {
                    self.blocks.push(new_block)
                } else {
                    return Err(BlockchainError::InvalidBlock((
                        last_block.clone(),
                        new_block,
                    )));
                }
            }
            None => self.blocks.push(new_block),
        };

        Ok(())
    }

    pub fn choose_chain<'a>(&'a self, remote: &'a Vec<Block>) -> &'a Vec<Block> {
        if is_chain_valid(&remote) {
            if self.blocks.len() >= remote.len() {
                &self.blocks
            } else {
                &remote
            }
        } else {
            &self.blocks
        }
    }

    pub fn contains_tx(&self, tx_id: &str) -> bool {
        self.blocks
            .iter()
            .any(|block| block.transactions.contains_tx(tx_id))
    }

    pub fn get_tx(&self, tx_id: &str) -> Option<&Transaction> {
        for block in &self.blocks {
            if let Some(tx) = block.transactions.get_tx(tx_id) {
                return Some(tx);
            }
        }
        None
    }

    pub fn prev_hash(&self) -> String {
        if let Some(last_block) = self.last() {
            hash_to_hex_string(&last_block.hash())
        } else {
            "Genesis".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mine_one_block() {
        let block = Block::from_mining(1, 0, "00".repeat(32).to_string(), TxBook::default());
        assert_eq!(block.get_nonce(), 86);
    }
}
