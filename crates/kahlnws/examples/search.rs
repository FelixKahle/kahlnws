// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

//! A short search over a network held: each move changes a few numbers, the network is solved, and the move is kept
//! where the cost falls below the best so far and set back where it does not.
//!
//! The network of the example `transport` for its first day: two warehouses hold 20 and 30 units, three shops ask for
//! 10, 25 and 15, every lane carries up to 20, and a unit costs 8, 6 and 10 from warehouse 0 to the shops and 9, 12 and
//! 13 from warehouse 1. The search tries five moves: lane 3 switched off by a capacity of zero; lane 1 made cheaper
//! under a contract to carry more than it can, a move of two changes whose second is refused; five units of stock
//! moved from warehouse 1 to warehouse 0; warehouse 0 set to hold 15 units; and lane 4 made cheaper. A move that is set
//! back is undone by its changes set back, each with the number its setter answered; a refused change changes nothing,
//! so the changes of a move made before it are set back the same way. Each move prints what it came to, and the end the
//! cost of the moves kept, with the stock of each warehouse read back from the network.
//!
//! Run it from the repository root with `cargo run -p kahlnws --example search`.

use kahlnws::{ChangeError, Network, Problem};

/// A change of the problem a network holds, by its setter: the lane or the nodes, and the number set.
#[derive(Clone, Copy, Debug)]
enum Change {
    /// The cost of a unit of flow on a lane.
    Cost { lane: usize, cost: i64 },
    /// The least flow of a lane.
    LowerBound { lane: usize, lower_bound: u64 },
    /// The most flow of a lane; zero switches it off.
    Capacity { lane: usize, capacity: u64 },
    /// The supply a node is to have, balanced by another node, whose supply moves the other way.
    Supply { node: usize, supply: i64, balancing_node: usize },
    /// Units of supply taken from one node and given to another.
    MovedSupply { from_node: usize, to_node: usize, amount: u64 },
}

impl Change {
    /// Makes the change on `network`, and answers the change that sets it back.
    fn make(self, network: &mut Network) -> Result<Self, ChangeError> {
        Ok(match self {
            Self::Cost { lane, cost } => Self::Cost { lane, cost: network.set_cost(lane, cost)? },
            Self::LowerBound { lane, lower_bound } => {
                Self::LowerBound { lane, lower_bound: network.set_lower_bound(lane, lower_bound)? }
            }
            Self::Capacity { lane, capacity } => {
                Self::Capacity { lane, capacity: network.set_capacity(lane, capacity)? }
            }
            Self::Supply { node, supply, balancing_node } => {
                Self::Supply { node, supply: network.set_supply(node, supply, balancing_node)?, balancing_node }
            }
            Self::MovedSupply { from_node, to_node, amount } => {
                network.move_supply(from_node, to_node, amount)?;
                Self::MovedSupply { from_node: to_node, to_node: from_node, amount }
            }
        })
    }
}

/// Makes the changes of a move on `network` in order, and answers the changes that set them back, the last made
/// first. Where one is refused, it sets back those made before it and answers the refusal: the network then holds the
/// problem it held.
fn make_move(network: &mut Network, changes: &[Change]) -> Result<Vec<Change>, ChangeError> {
    let mut set_backs = Vec::with_capacity(changes.len());
    for change in changes {
        match change.make(network) {
            Ok(set_back) => set_backs.insert(0, set_back),
            Err(refusal) => {
                set_back(network, &set_backs)?;
                return Err(refusal);
            }
        }
    }

    Ok(set_backs)
}

/// Makes each change of `set_backs` on `network`, in order.
fn set_back(network: &mut Network, set_backs: &[Change]) -> Result<(), ChangeError> {
    for change in set_backs {
        change.make(network)?;
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Node w is warehouse w and node 2 + s shop s; lane 3 * w + s leads from warehouse w to shop s.
    let mut network = Network::new();
    network.load(&Problem {
        tails: &[0, 0, 0, 1, 1, 1],
        heads: &[2, 3, 4, 2, 3, 4],
        lower_bounds: None,
        capacities: &[20; 6],
        costs: &[8, 6, 10, 9, 12, 13],
        supplies: &[20, 30, -10, -25, -15],
    })?;

    // By hand: a flow costs what warehouse 1 would pay to serve every shop alone, less what each unit warehouse 0
    // sends in its place saves, 1, 6 and 3 at shops 0, 1 and 2 while the costs are those loaded. Warehouse 1 alone
    // would pay 9 * 10 + 12 * 25 + 13 * 15 = 585; warehouse 0's 20 units save 6 each at shop 1, whose lane from it
    // takes all 20: 465.
    let mut best = network.solve().outcome.optimal().expect("the warehouses should cover the demand");
    println!("start: cost {best}");
    assert_eq!(best, 585 - 6 * 20);

    let moves: [(&[Change], &str); 5] = [
        // Lane 3 off: warehouse 0 serves shop 0, saving 1 a unit, and has 10 units left for shop 1: 585 - 10 - 60.
        (&[Change::Capacity { lane: 3, capacity: 0 }], "cost 515, set back"),
        // Lane 1 at 4 a unit for at least 25 units: it carries at most 20, so the cost goes back to 6.
        (
            &[Change::Cost { lane: 1, cost: 4 }, Change::LowerBound { lane: 1, lower_bound: 25 }],
            "refused (arc 1 has lower bound 25 above its capacity 20), set back",
        ),
        // Warehouse 0 holds 25: 20 to shop 1 save 6 each and 5 to shop 2 save 3 each: 585 - 120 - 15.
        (&[Change::MovedSupply { from_node: 1, to_node: 0, amount: 5 }], "cost 450, kept"),
        // Warehouse 0 to hold 15, warehouse 1 the other 35: all 15 to shop 1 save 6 each: 585 - 90.
        (&[Change::Supply { node: 0, supply: 15, balancing_node: 1 }], "cost 495, set back"),
        // Lane 4 at 7: warehouse 1 alone pays 460; 15 units to shop 2 save 3 each, the other 10 1 each: 460 - 55.
        (&[Change::Cost { lane: 4, cost: 7 }], "cost 405, kept"),
    ];

    for (number, (changes, hand_worked)) in (1..).zip(moves) {
        let came_to = match make_move(&mut network, changes) {
            Err(refusal) => format!("refused ({refusal}), set back"),
            Ok(set_backs) => match network.solve().outcome.optimal() {
                Some(cost) if cost < best => {
                    best = cost;
                    format!("cost {cost}, kept")
                }
                cost => {
                    set_back(&mut network, &set_backs)?;
                    cost.map_or_else(|| "no flow, set back".to_owned(), |cost| format!("cost {cost}, set back"))
                }
            },
        };
        println!("move {number}: {came_to}");
        assert_eq!(came_to, hand_worked, "move {number}");
    }

    // The moves kept: warehouse 0 holds 25 and warehouse 1 25, lane 4 costs 7, every other number is as loaded.
    let end = network.solve().outcome.optimal().expect("the moves kept should leave a flow");
    let (stock_0, stock_1) = (network.supply(0), network.supply(1));
    println!("end: cost {end}, the warehouses hold {stock_0} and {stock_1}");
    assert_eq!((end, stock_0, stock_1), (best, 25, 25));

    Ok(())
}
