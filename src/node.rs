use chrono::Utc;

use crate::blockchain::{hash_to_hex_string, Block, Blockchain, DIFFICULTY_PREFIX};
use crate::transaction::TxBook;
use std::collections::HashSet;
use std::sync::mpsc::SendError;
use std::sync::{mpsc, Arc, Mutex};
#[derive(Debug)]
pub struct Node {
    /// Mining entity. A node has a local blockchain, and a set of current transactions for which
    /// it is mining. The set of local transactions may be updated when a set of new transactions is broadcasted.
    /// Mining is ran in a thread, and the blockchain is ultimately sent to a master via a channel.
    local_bc: Blockchain,
    current_txs: TxBook,
    nonce_start: u64,
    stop_signal: Arc<Mutex<bool>>,
    tx_channel: mpsc::Receiver<TxBook>,
    bc_channel: mpsc::Sender<Blockchain>,
}

impl Node {
    pub fn new(
        nonce_start: u64,
        stop_signal: Arc<Mutex<bool>>,
        tx_channel: mpsc::Receiver<TxBook>,
        bc_channel: mpsc::Sender<Blockchain>,
    ) -> Self {
        Self {
            local_bc: Blockchain::new(),
            current_txs: TxBook::default(),
            nonce_start: nonce_start,
            stop_signal,
            tx_channel,
            bc_channel,
        }
    }

    fn update_local_txs(&mut self, tx_list: &TxBook) {
        tx_list
            .book
            .iter()
            .for_each(|(_, tx)| self.current_txs.add_tx(tx.clone()));
        tx_list
            .utxos
            .iter()
            .for_each(|tx_id| self.current_txs.add_utxo(tx_id));
    }

    pub fn extract_valid_txs(&self, tx_list: &TxBook) -> TxBook {
        let mut utxos: HashSet<String> = tx_list.utxos.clone();
        let mut valid_tx: TxBook = TxBook::default();
        let mut seen: HashSet<String> = HashSet::new();
        while let Some(utxo) = utxos.iter().next().cloned() {
            utxos.remove(&utxo);
            let mut to_see: HashSet<String> = HashSet::new();
            to_see.insert(utxo);

            while let Some(current_tx_id) = to_see.iter().next().cloned() {
                to_see.remove(&current_tx_id);
                let current_tx = tx_list.get_tx(&current_tx_id).unwrap();
                if !seen.contains(&current_tx_id) {
                    // Check that all inputs of transaction match an output of an existing transaction
                    let all_inputs_match: bool = current_tx.inputs.iter().all(|input| {
                        if let Some(tx_prev_block) = self.local_bc.get_tx(&input.origin_tx) {
                            tx_prev_block.input_matches_output(input)
                        } else if let Some(tx_current_chain) = tx_list.get_tx(&input.origin_tx) {
                            tx_current_chain.input_matches_output(input)
                        } else {
                            false
                        }
                    });
                    // Everything we need to check for a transaction to be added to the block
                    if all_inputs_match
                        && current_tx.has_valid_balance()
                        && current_tx.has_valid_inputs()
                        && current_tx.has_valid_signature()
                    {
                        current_tx.inputs.iter().for_each(|input| {
                            if !self.local_bc.contains_tx(&input.origin_tx) {
                                to_see.insert(input.origin_tx.to_string());
                            }
                        });
                        valid_tx.add_tx(current_tx.to_owned());
                    }
                } else {
                    if !valid_tx.contains_tx(&current_tx_id) {
                        valid_tx.clear_tx(&current_tx_id)
                    }
                }
                seen.insert(current_tx_id);
            }
        }
        valid_tx.utxos = valid_tx.find_utxos();
        valid_tx
    }

    pub fn receive_block(
        &mut self,
        block: &Block,
    ) -> Result<(), crate::blockchain::BlockchainError> {
        if self.extract_valid_txs(&block.transactions) == block.transactions {
            // If a block is accepted, we reset the transactions locally saved on the node
            self.current_txs.book.drain();
            self.current_txs.utxos.drain();
            self.local_bc.try_add_block(block.clone())
        } else {
            Ok(())
        }
    }

    pub fn start_mining(&mut self) -> Result<(), SendError<Blockchain>> {
        let mut candidate_block = Block::new(
            0,
            0,
            &self.local_bc.prev_hash(),
            self.nonce_start,
            &self.current_txs,
        );
        loop {
            if let Ok(tx_list) = self.tx_channel.try_recv() {
                self.update_local_txs(&tx_list);
                let timestamp = Utc::now().timestamp();
                let id = (self.local_bc.len() + 1) as u64;
                candidate_block = Block::new(
                    id,
                    timestamp,
                    &self.local_bc.prev_hash(),
                    self.nonce_start,
                    &self.current_txs,
                );
            }
            if !self.current_txs.book.is_empty() {
                candidate_block.set_nonce(candidate_block.get_nonce() + 1);
                if hash_to_hex_string(&candidate_block.hash()).starts_with(DIFFICULTY_PREFIX) {
                    self.current_txs = TxBook::default();
                    self.local_bc
                        .try_add_block(candidate_block.clone())
                        .unwrap();
                }
            }
            if *self.stop_signal.lock().unwrap() && self.current_txs.book.is_empty() {
                self.bc_channel.send(self.local_bc.clone())?;
                break;
            }
        }
        Ok(())
    }
}

pub struct NodeMaster {
    stop_signal: Arc<Mutex<bool>>,
    tx_senders: Vec<mpsc::Sender<TxBook>>,
    bc_receivers: Vec<mpsc::Receiver<Blockchain>>,
}

impl NodeMaster {
    pub fn new(n: u8) -> (Self, Vec<Node>) {
        let stop_signal = Arc::new(Mutex::new(false));
        let mut tx_senders: Vec<mpsc::Sender<TxBook>> = Vec::new();
        let mut bc_receivers: Vec<mpsc::Receiver<Blockchain>> = Vec::new();
        let mut nodes: Vec<Node> = Vec::new();
        (0..n).for_each(|i| {
            let (tx_sender, tx_receiver) = mpsc::channel();
            let (bc_sender, bc_receiver) = mpsc::channel();
            bc_receivers.push(bc_receiver);
            tx_senders.push(tx_sender);
            nodes.push(Node::new(
                u64::MAX / (n as u64) * (i as u64),
                stop_signal.clone(),
                tx_receiver,
                bc_sender,
            ));
        });
        (
            Self {
                stop_signal,
                tx_senders,
                bc_receivers,
            },
            nodes,
        )
    }

    pub fn broadcast_txlist(&self, tx_list: &TxBook) -> Result<(), SendError<TxBook>> {
        for tx_sender in &self.tx_senders {
            tx_sender.send(tx_list.clone())?;
        }
        Ok(())
    }

    pub fn stop_nodes(&self) {
        let mut stop = self.stop_signal.lock().unwrap();
        *stop = true;
    }

    pub fn receive_bc(&self) -> Result<Vec<Blockchain>, mpsc::RecvError> {
        self.bc_receivers
            .iter()
            .map(|bc_receiver| bc_receiver.recv())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::*;
    use crate::user::TxGenerator;

    #[test]
    fn test_extraction_full_honesty() {
        let mut generator = TxGenerator::new(2, 100);
        generator.execute_tx(0, 1, 50);
        generator.execute_tx(1, 0, 25);
        let stop_signal = Arc::new(Mutex::new(false));
        let (_, tx_receiver) = mpsc::channel();
        let (bc_sender, _) = mpsc::channel();
        let node = Node::new(0, stop_signal, tx_receiver, bc_sender);
        let extracted_txbook = node.extract_valid_txs(&generator.txbook);

        assert_eq!(generator.txbook.book, extracted_txbook.book);
    }

    #[test]
    fn test_single_node_mining() {
        let mut generator = TxGenerator::new(2, 100);

        let stop_signal = Arc::new(Mutex::new(false));
        let (tx_sender, tx_receiver) = mpsc::channel();
        let (bc_sender, bc_receiver) = mpsc::channel();
        let mut node = Node::new(0, Arc::clone(&stop_signal), tx_receiver, bc_sender);
        let handle = thread::spawn(move || node.start_mining());
        generator.execute_tx(0, 1, 50);
        generator.execute_tx(1, 0, 25);
        let tx_list = generator.broadcast_txs();
        tx_sender.send(tx_list.clone()).unwrap();
        {
            let mut stop = stop_signal.lock().unwrap();
            *stop = true;
        }
        handle.join().unwrap().unwrap();
        let received_bc = bc_receiver.recv();
        assert!(received_bc.is_ok());
        let blockchain = received_bc.unwrap();

        assert!(blockchain.last().is_some());
        let last_block = blockchain.last().unwrap();
        assert_eq!(last_block.transactions, tx_list);
        assert!(hash_to_hex_string(&last_block.hash()).starts_with(DIFFICULTY_PREFIX));
    }
}
