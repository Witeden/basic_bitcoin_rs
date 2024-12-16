/// Users are distinct from nodes, which are the mining entities. Users are the one producing transactions.
use rand::Rng;
use secp256k1::{generate_keypair, Message, PublicKey, SecretKey};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use crate::transaction::{Input, Output, Transaction, TxBook};

#[derive(Debug)]
pub struct User {
    wallet: Vec<Transaction>,
    balance: u64,
    pub pubkey: PublicKey,
    seckey: SecretKey,
}

impl PartialEq for User {
    fn eq(&self, other: &Self) -> bool {
        self.pubkey == other.pubkey
    }
}

impl User {
    pub fn new() -> Self {
        let (seckey, pubkey) = generate_keypair(&mut rand::thread_rng());
        let user = User {
            wallet: Vec::new(),
            balance: 0,
            pubkey,
            seckey,
        };
        user
    }
    pub fn insert_tx(&mut self, tx: Transaction) {
        self.wallet.push(tx);
    }

    pub fn create_tx(&mut self, receiver_key: &str, value: u64) -> Option<(String, Transaction)> {
        if self.balance >= value {
            let mut inputs: Vec<Input> = Vec::new();
            let mut inputs_sum = 0;
            let mut tx_iter = self.wallet.iter();
            while inputs_sum < value {
                if let Some(current_tx) = tx_iter.next() {
                    let payment_key = PublicKey::from_str(&current_tx.payment.pubkey)
                        .unwrap_or_else(|_| panic!("Publick key of output can't be parsed."));
                    let payment_type = if payment_key == self.pubkey {
                        inputs_sum += current_tx.payment.value;
                        true
                    } else {
                        inputs_sum += current_tx.change.value;
                        false
                    };
                    let new_input = Input::from_output_tx(&current_tx, payment_type);
                    inputs.push(new_input);
                } else {
                    panic!("Balance of user does not match sum of all transactions of wallet.")
                }
            }
            let mut to_hash = inputs
                .iter()
                .map(|input| input.origin_tx.clone())
                .collect::<Vec<String>>()
                .join("_");
            to_hash = vec![to_hash, receiver_key.to_string()].join("_");
            let mut hasher = Sha256::new();
            hasher.update(to_hash.as_bytes());
            let hash_bytes: [u8; 32] = hasher
                .finalize()
                .as_slice()
                .to_owned()
                .try_into()
                .unwrap_or_else(|_| unreachable!("SHA256 has a 32 bytes output"));
            let to_sign = Message::from_digest(hash_bytes);
            let sig = self.seckey.sign_ecdsa(to_sign);
            let new_tx = Transaction::new(
                inputs,
                Output::new(&receiver_key.to_string(), value),
                Output::new(&self.pubkey.to_string(), inputs_sum - value),
                &sig.to_string(),
            );
            if inputs_sum - value > 0 {
                self.wallet.push(new_tx.clone());
            }
            self.balance -= value;
            Some((new_tx.hash(), new_tx))
        } else {
            None
        }
    }

    pub fn receive_tx(&mut self, tx: Transaction) {
        self.balance += tx.payment.value;
        self.wallet.push(tx);
    }
}

pub struct TxGenerator {
    /// Artificial structure that allows to generate transactions between users.
    /// The generator generates an arbitrary number of valid transactions between users, and
    /// broadcast the set of generated transactions to nodes.
    /// Once the generator broadcasts the transactions, the book of transactions
    /// is emptied.
    users: Vec<User>,
    pub txbook: TxBook,
}

impl TxGenerator {
    pub fn new(n: u8, individual_balance: u64) -> Self {
        let mut txbook = TxBook::default();
        let mut users: Vec<User> = (0..n.max(1)).map(|_| User::new()).collect();
        users.iter_mut().for_each(|user| {
            let init_tx = Transaction::new(
                Vec::new(),
                Output::new(&user.pubkey.to_string(), individual_balance),
                Output::new("", 0),
                "Authority transaction",
            );
            user.insert_tx(init_tx.clone());
            user.balance += individual_balance;
            let tx_id = init_tx.hash();
            txbook.add_tx(init_tx);
            txbook.add_utxo(&tx_id);
        });
        TxGenerator { users, txbook }
    }

    pub fn execute_tx(&mut self, payer_idx: usize, receiver_idx: usize, value: u64) {
        let receiver_key = self.users[receiver_idx].pubkey.to_string();
        let payer = &mut self.users[payer_idx];
        if let Some((_, tx)) = payer.create_tx(&receiver_key, value) {
            let receiver = &mut self.users[receiver_idx];
            receiver.receive_tx(tx.clone());
            let tx_id = tx.hash();
            self.txbook.add_tx(tx);
            self.txbook.add_utxo(&tx_id);
        }
    }

    fn pick_random_user(&self) -> usize {
        rand::thread_rng().gen_range(0..self.users.len()) as usize
    }

    pub fn generate_random_tx(&mut self) {
        loop {
            let mut payer_idx = self.pick_random_user();
            let mut payer_balance = self.users[payer_idx].balance;
            while self.users[payer_idx].balance == 0 {
                payer_idx = self.pick_random_user();
                payer_balance = self.users[payer_idx].balance;
            }
            let value = rand::thread_rng().gen_range(1..=payer_balance);
            let mut receiver_idx = self.pick_random_user();
            while receiver_idx == payer_idx {
                receiver_idx = self.pick_random_user();
            }
            self.execute_tx(payer_idx, receiver_idx, value);
        }
    }

    pub fn broadcast_txs(&mut self) -> TxBook {
        let mut broadcast_book: HashMap<String, Transaction> = HashMap::new();
        broadcast_book.extend(self.txbook.book.drain());
        let mut broadcast_utxos: HashSet<String> = HashSet::new();
        broadcast_utxos.extend(self.txbook.utxos.drain());

        TxBook::new(broadcast_book, broadcast_utxos)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execute_one_tx() {
        let mut generator = TxGenerator::new(2, 100);
        generator.execute_tx(0, 1, 50);
        assert_eq!(50, generator.users[0].balance);
        assert_eq!(150, generator.users[1].balance);
        assert_eq!(3, generator.txbook.n_tx());
    }
}
