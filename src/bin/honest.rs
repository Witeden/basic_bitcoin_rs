use basic_bitcoin_rs::{node::NodeMaster, user::TxGenerator};
use std::{error::Error, thread, time::Duration};

/// Binary to illustrate the functionnalities of this library.
/// All users and all nodes are honest.

pub fn main() -> Result<(), Box<dyn Error>> {
    let mut generator = TxGenerator::new(4, 100);
    let (master, nodes) = NodeMaster::new(5);
    let handles: Vec<_> = nodes
        .into_iter()
        .map(|mut node| thread::spawn(move || node.start_mining()))
        .collect();

    for _ in 0..4 {
        generator.generate_random_tx();
    }
    let tx_list = generator.broadcast_txs();
    println!("Broadcasting first batch of transactions...");
    master
        .broadcast_txlist(&tx_list)
        .map_err(|e| Box::new(e) as Box<dyn Error>)?;
    println!("Let the nodes work...");
    thread::sleep(Duration::from_secs(1));

    for _ in 0..4 {
        generator.generate_random_tx();
    }
    let tx_list = generator.broadcast_txs();
    println!("Broadcasting second batch of transactions...");
    master
        .broadcast_txlist(&tx_list)
        .map_err(|e| Box::new(e) as Box<dyn Error>)?;

    master.stop_nodes();
    for handle in handles {
        handle
            .join()
            .unwrap()
            .map_err(|e| Box::new(e) as Box<dyn Error>)?;
    }
    println!("Receiving blockchains from nodes");
    let blockchains = master
        .receive_bc()
        .map_err(|e| Box::new(e) as Box<dyn Error>)?;
    if let Some(first_bc) = blockchains.first() {
        if blockchains.iter().all(|bc| {
            bc.blocks
                .iter()
                .zip(first_bc.blocks.iter())
                .all(|(rhs, lhs)| rhs.transactions == lhs.transactions)
        }) {
            println!("All blockchains have same transactions set in each block.");
            println!("Each blockchain is of length {}", first_bc.len());
        } else {
            println!("All nodes didn't compute the same blockchain.")
        }
    } else {
        println!("Received no blockchain.");
    }
    Ok(())
}
