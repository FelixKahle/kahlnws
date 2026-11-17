// Copyright 2026 Felix Kahle
// SPDX-License-Identifier: MIT OR Apache-2.0

// A short search over a network held: the network of `transport.cpp` for its first day, loaded once and solved; then
// five moves, each changing a few numbers, solved, and kept where the cost falls below the best so far or set back
// where it does not: lane 3 switched off by a capacity of zero; lane 1 made cheaper under a contract to carry more than
// it can, a move of two changes whose second is refused; five units of stock moved from warehouse 1 to warehouse 0;
// warehouse 0 set to hold 15 units; and lane 4 made cheaper. A move that is set back is undone by its changes set
// back, each with the number its setter answered; a refused change changes nothing, so the changes of a move made
// before it are set back the same way. The program prints what each move came to, and the cost of the moves kept with
// the stock of each warehouse read back from the network, and checks each line against the one worked out by hand. It
// uses the forms that answer errors as values, so it builds with exceptions enabled or not.
//
// Built and run from the repository's root, against the static library (on Linux, the libraries
// `cargo rustc --release -p kahlnws_capi --lib --crate-type staticlib -- --print native-static-libs` names follow it):
//
//     cargo build --release -p kahlnws_capi
//     c++ -std=c++11 -Wall -Wextra -Wpedantic -Werror -I crates/kahlnws_capi/include \
//         crates/kahlnws_capi/examples/search.cpp target/release/libkahlnws_capi.a -o search
//     ./search

#include <cstddef>
#include <cstdint>
#include <functional>
#include <iostream>
#include <string>
#include <utility>
#include <vector>

#include "kahlnws.hpp"

namespace {

/// A change made, ready to be set back: it makes the change back on a network, and answers what its setter answered.
using change_back = std::function<kahlnws::result<void>(kahlnws::network&)>;

/// A change of the problem a network holds: it makes itself on a network, and answers the change that sets it back,
/// or the error of its setter.
using change = std::function<kahlnws::result<change_back>(kahlnws::network&)>;

/// A move of the search: its changes, made in order, and what it comes to, worked out by hand.
struct move {
    std::vector<change> changes;
    std::string hand_worked;
};

/// What a setter answered, without the number it replaced.
template <class Number>
kahlnws::result<void> without_number(const kahlnws::result<Number>& answered) {
    if (!answered) {
        return answered.error();
    }
    return kahlnws::result<void>();
}

/// The change that makes `cost` the cost of a unit of flow on `lane`.
change lane_cost(std::size_t lane, std::int64_t cost) {
    return [lane, cost](kahlnws::network& network) -> kahlnws::result<change_back> {
        const kahlnws::result<std::int64_t> had = network.try_set_cost(lane, cost);
        if (!had) {
            return had.error();
        }
        const std::int64_t back = *had;
        return change_back([lane, back](kahlnws::network& again) {
            return without_number(again.try_set_cost(lane, back));
        });
    };
}

/// The change that makes `lower_bound` the least flow of `lane`.
change lane_lower_bound(std::size_t lane, std::uint64_t lower_bound) {
    return [lane, lower_bound](kahlnws::network& network) -> kahlnws::result<change_back> {
        const kahlnws::result<std::uint64_t> had = network.try_set_lower_bound(lane, lower_bound);
        if (!had) {
            return had.error();
        }
        const std::uint64_t back = *had;
        return change_back([lane, back](kahlnws::network& again) {
            return without_number(again.try_set_lower_bound(lane, back));
        });
    };
}

/// The change that makes `capacity` the most flow of `lane`: zero switches it off.
change lane_capacity(std::size_t lane, std::uint64_t capacity) {
    return [lane, capacity](kahlnws::network& network) -> kahlnws::result<change_back> {
        const kahlnws::result<std::uint64_t> had = network.try_set_capacity(lane, capacity);
        if (!had) {
            return had.error();
        }
        const std::uint64_t back = *had;
        return change_back([lane, back](kahlnws::network& again) {
            return without_number(again.try_set_capacity(lane, back));
        });
    };
}

/// The change that makes `supply` the supply of `node`, balanced by `balancing_node`, whose supply moves the other way.
change node_supply(std::size_t node, std::int64_t supply, std::size_t balancing_node) {
    return [node, supply, balancing_node](kahlnws::network& network) -> kahlnws::result<change_back> {
        const kahlnws::result<std::int64_t> had = network.try_set_supply(node, supply, balancing_node);
        if (!had) {
            return had.error();
        }
        const std::int64_t back = *had;
        return change_back([node, back, balancing_node](kahlnws::network& again) {
            return without_number(again.try_set_supply(node, back, balancing_node));
        });
    };
}

/// The change that takes `amount` units from the supply of `from_node` and gives them to `to_node`.
change moved_supply(std::size_t from_node, std::size_t to_node, std::uint64_t amount) {
    return [from_node, to_node, amount](kahlnws::network& network) -> kahlnws::result<change_back> {
        const kahlnws::result<void> moved = network.try_move_supply(from_node, to_node, amount);
        if (!moved) {
            return moved.error();
        }
        return change_back([from_node, to_node, amount](kahlnws::network& again) {
            return again.try_move_supply(to_node, from_node, amount);
        });
    };
}

/// Makes each change of `changes_back` on `network`, in order.
kahlnws::result<void> set_back(kahlnws::network& network, const std::vector<change_back>& changes_back) {
    for (const change_back& back : changes_back) {
        const kahlnws::result<void> made = back(network);
        if (!made) {
            return made;
        }
    }
    return kahlnws::result<void>();
}

/// Makes `changes` on `network` in order, and answers the changes that set them back, the last made first. Where one
/// is refused, it sets back those made before it and answers the refusal: the network then holds the problem it held.
kahlnws::result<std::vector<change_back> > make_move(kahlnws::network& network, const std::vector<change>& changes) {
    std::vector<change_back> changes_back;
    for (const change& wanted : changes) {
        kahlnws::result<change_back> made = wanted(network);
        if (!made) {
            const kahlnws::result<void> undone = set_back(network, changes_back);
            return undone ? made.error() : undone.error();
        }
        changes_back.insert(changes_back.begin(), std::move(*made));
    }
    return changes_back;
}

}  // namespace

int main() {
    // Node w is warehouse w and node 2 + s shop s; lane 3 * w + s leads from warehouse w to shop s.
    const std::vector<std::size_t> tails = {0, 0, 0, 1, 1, 1};
    const std::vector<std::size_t> heads = {2, 3, 4, 2, 3, 4};
    const std::vector<std::uint64_t> capacities = {20, 20, 20, 20, 20, 20};
    const std::vector<std::int64_t> costs = {8, 6, 10, 9, 12, 13};
    const std::vector<std::int64_t> supplies = {20, 30, -10, -25, -15};

    // By hand: a flow costs what warehouse 1 would pay to serve every shop alone, less what each unit warehouse 0
    // sends in its place saves, 1, 6 and 3 at shops 0, 1 and 2 while the costs are those loaded. Warehouse 1 alone
    // would pay 9 * 10 + 12 * 25 + 13 * 15 = 585; warehouse 0's 20 units save 6 each at shop 1, whose lane from it
    // takes all 20: 465.
    const std::string start = "start: cost 465";
    const std::vector<move> moves = {
        // Lane 3 off: warehouse 0 serves shop 0, saving 1 a unit, and has 10 units left for shop 1: 585 - 10 - 60.
        {{lane_capacity(3, 0)}, "cost 515, set back"},
        // Lane 1 at 4 a unit for at least 25 units: it carries at most 20, so the cost goes back to 6.
        {{lane_cost(1, 4), lane_lower_bound(1, 25)},
         "refused (arc 1 has lower bound 25 above its capacity 20), set back"},
        // Warehouse 0 holds 25: 20 to shop 1 save 6 each and 5 to shop 2 save 3 each: 585 - 120 - 15.
        {{moved_supply(1, 0, 5)}, "cost 450, kept"},
        // Warehouse 0 to hold 15, warehouse 1 the other 35: all 15 to shop 1 save 6 each: 585 - 90.
        {{node_supply(0, 15, 1)}, "cost 495, set back"},
        // Lane 4 at 7: warehouse 1 alone pays 460; 15 units to shop 2 save 3 each, the other 10 1 each: 460 - 55.
        {{lane_cost(4, 7)}, "cost 405, kept"},
    };
    // The moves kept: warehouse 0 holds 25 and warehouse 1 25, lane 4 costs 7, every other number is as loaded.
    const std::string end = "end: cost 405, the warehouses hold 25 and 25";

    kahlnws::problem problem;
    problem.tails = tails;
    problem.heads = heads;
    problem.capacities = capacities;
    problem.costs = costs;
    problem.supplies = supplies;

    kahlnws::network network;
    const kahlnws::result<void> loaded = network.try_load(problem);
    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > started = network.try_solve();
    if (!loaded || !started || started->outcome.optimal() == nullptr) {
        std::cerr << "the first day should load and solve\n";
        return 1;
    }
    kahlnws::int128 best = started->outcome.optimal()->total_cost;
    const std::string started_at = "start: cost " + kahlnws::to_string(best);
    std::cout << started_at << '\n';
    if (started_at != start) {
        std::cerr << "the start should print \"" << start << "\"\n";
        return 1;
    }

    for (std::size_t number = 0; number < moves.size(); ++number) {
        const kahlnws::result<std::vector<change_back> > made = make_move(network, moves[number].changes);
        std::string came_to;
        if (!made && made.error().kind() == kahlnws::error_kind::refused_change) {
            came_to = std::string("refused (") + made.error().message() + "), set back";
        } else if (!made) {
            std::cerr << "move " << number + 1 << ": " << made.error().message() << '\n';
            return 1;
        } else {
            const kahlnws::result<kahlnws::answer<kahlnws::optimum> > answered = network.try_solve();
            if (!answered) {
                std::cerr << "move " << number + 1 << ": " << answered.error().message() << '\n';
                return 1;
            }
            const kahlnws::optimum* const optimum = answered->outcome.optimal();
            if (optimum != nullptr && optimum->total_cost < best) {
                best = optimum->total_cost;
                came_to = "cost " + kahlnws::to_string(best) + ", kept";
            } else if (!set_back(network, *made)) {
                std::cerr << "move " << number + 1 << ": the changes should be set back\n";
                return 1;
            } else {
                came_to = optimum == nullptr ? "no flow" : "cost " + kahlnws::to_string(optimum->total_cost);
                came_to += ", set back";
            }
        }

        std::cout << "move " << number + 1 << ": " << came_to << '\n';
        if (came_to != moves[number].hand_worked) {
            std::cerr << "move " << number + 1 << " should come to \"" << moves[number].hand_worked << "\"\n";
            return 1;
        }
    }

    const kahlnws::result<kahlnws::answer<kahlnws::optimum> > ended = network.try_solve();
    const kahlnws::result<std::int64_t> stock_0 = network.try_supply(0);
    const kahlnws::result<std::int64_t> stock_1 = network.try_supply(1);
    if (!ended || ended->outcome.optimal() == nullptr || !stock_0 || !stock_1) {
        std::cerr << "the moves kept should leave a flow\n";
        return 1;
    }
    const std::string ended_at = "end: cost " + kahlnws::to_string(ended->outcome.optimal()->total_cost) +
                                 ", the warehouses hold " + std::to_string(*stock_0) + " and " +
                                 std::to_string(*stock_1);
    std::cout << ended_at << '\n';
    if (ended_at != end) {
        std::cerr << "the end should print \"" << end << "\"\n";
        return 1;
    }
    return 0;
}
