//! The market's weeks: the player's moves and offers ([`market_moves`], [`answer_offer`])
//! and the rival clubs' releases, signings and purchases ([`rival_market`]).

use std::cmp::Reverse;

use rand::RngExt;

use crate::colosseum::Contender;
use crate::design_data::DesignData;
use crate::economy::{Flow, Transaction, Wealth};
use crate::fighter::career::{CareerFighter, Status};
use crate::fighter::percent_of;
use crate::market::{self, MarketNews, Move, SaleOffer, estimated_potential, value};
use crate::relations::Reason;
use crate::rivals::{Eye, RivalId};

use super::clubs::off_season_starts;
use super::{GameState, News, WorldNews, relate};

/// The market moves made before the week, in this order, each if still allowed: the
/// first observation, accepted offers, releases, the transfer list, then signings. Last
/// week's counter-offer closes.
pub(super) fn market_moves(
    state: &mut GameState,
    moves: &[Move],
    data: &DesignData,
    ledger: &mut Vec<Transaction>,
) -> Vec<MarketNews> {
    state.counter_offer = None;
    let mut news = Vec::new();
    if let Some(target) = moves.iter().find_map(|&m| match m {
        Move::Observe(target) => Some(target),
        _ => None,
    }) && let Some(fighter) = state.target_mut(target)
    {
        fighter.observed_weeks += 1;
    }
    let order = |m: &Move| match *m {
        Move::AcceptOffer { .. } => 0,
        Move::Release(_) => 1,
        Move::List(_) | Move::Unlist(_) | Move::Keep(_) | Move::Unkeep(_) => 2,
        Move::Sign(_) => 3,
        Move::Observe(_) | Move::Offer(..) => u8::MAX,
    };
    let mut moves: Vec<Move> = moves.to_vec();
    moves.sort_by_key(order);
    for m in moves {
        if order(&m) == u8::MAX || !state.may_move(m, data) {
            continue;
        }
        match m {
            Move::AcceptOffer { fighter, rival } => {
                let Some(&offer) = state
                    .sale_offers
                    .iter()
                    .find(|o| o.fighter == fighter && o.rival == rival)
                else {
                    continue;
                };
                let mut sold = state.fighters[fighter].clone();
                sold.joined = Some(state.week);
                state.fighters[fighter].status = Status::Sold;
                state.rivals[rival].fighters.push(sold);
                let to = RivalId {
                    club: rival,
                    fighter: state.rivals[rival].fighters.len() - 1,
                };
                state.transfer(Contender::Club(fighter), Contender::Rival(to));
                let price = i64::from(offer.price);
                state.rivals[rival].money -= price;
                state.money += price;
                ledger.push(Transaction {
                    fighter: Some(fighter),
                    flow: Flow::Sale,
                    amount: price,
                });
                state.sale_offers.retain(|o| o.fighter != fighter);
                relate(state, rival, Reason::Sold { fighter }, data);
                news.push(MarketNews::Sold(offer));
            }
            Move::Release(i) => {
                let released = state.fighters[i].clone();
                state.fighters[i].status = Status::Released;
                state.free_agents.push(released);
                let to = Contender::FreeAgent(state.free_agents.len() - 1);
                state.transfer(Contender::Club(i), to);
                news.push(MarketNews::Released(i));
            }
            Move::List(i) => state.transfer_list.push(i),
            Move::Unlist(i) => state.transfer_list.retain(|&l| l != i),
            Move::Keep(i) => state.kept.push(i),
            Move::Unkeep(i) => state.kept.retain(|&k| k != i),
            Move::Sign(a) => {
                let mut agent = state.free_agents[a].clone();
                agent.joined = Some(state.week);
                state.free_agents[a].status = Status::Sold;
                state.fighters.push(agent);
                let i = state.fighters.len() - 1;
                state.transfer(Contender::FreeAgent(a), Contender::Club(i));
                news.push(MarketNews::Signed(i));
            }
            Move::Observe(_) | Move::Offer(..) => {}
        }
    }
    let active = |i: &usize| state.fighters[*i].status == Status::Active;
    state.transfer_list.retain(active);
    state.kept.retain(active);
    news
}

/// The answer to the first offer of `moves`, if still allowed (see `negotiate`).
pub(super) fn answer_offer(
    state: &mut GameState,
    moves: &[Move],
    data: &DesignData,
    ledger: &mut Vec<Transaction>,
) -> Option<MarketNews> {
    let (id, price) = moves.iter().find_map(|&m| match m {
        Move::Offer(id, price) => Some((id, price)),
        _ => None,
    })?;
    state
        .may_move(Move::Offer(id, price), data)
        .then(|| negotiate(state, id, price, data, ledger))
}

/// The answer to the player's offer of `price` for rival fighter `id`, which
/// `GameState::may_move` allows. At the club's asking price or more, the club accepts
/// and the fighter decides (`market::refuses_to_join`); joining, it becomes the
/// club's, paid for in `ledger`. From `counter_percent` of it, the club asks its price
/// for the next week; below, it refuses.
fn negotiate(
    state: &mut GameState,
    id: RivalId,
    price: u32,
    data: &DesignData,
    ledger: &mut Vec<Transaction>,
) -> MarketNews {
    let market = &data.economy.market;
    let Some(asking) = state.price_to_player(id, data) else {
        return MarketNews::Failed(id);
    };
    if price < asking {
        let counter =
            u64::from(price) * 100 >= u64::from(asking) * u64::from(market.counter_percent);
        if !counter {
            return MarketNews::ClubRefused(id);
        }
        state.counter_offer = Some((id, asking));
        return MarketNews::Countered(id, asking);
    }
    let club_fame = state.club_fame();
    let fighter = &state.rivals[id.club].fighters[id.fighter];
    if market::refuses_to_join(&mut state.rng, fighter, club_fame, market) {
        return MarketNews::FighterRefused(id);
    }
    if !state.can_hire(data) || i64::from(price) > state.money {
        return MarketNews::Failed(id);
    }
    let mut bought = fighter.clone();
    bought.joined = Some(state.week);
    state.rivals[id.club].fighters[id.fighter].status = Status::Sold;
    state.fighters.push(bought);
    let amount = i64::from(price);
    state.money -= amount;
    state.rivals[id.club].money += amount;
    let fighter = state.fighters.len() - 1;
    state.transfer(Contender::Rival(id), Contender::Club(fighter));
    ledger.push(Transaction {
        fighter: Some(fighter),
        flow: Flow::Purchase,
        amount: -amount,
    });
    relate(state, id.club, Reason::Bought { fighter }, data);
    MarketNews::Bought {
        from: id,
        fighter,
        price,
    }
}

/// The rival clubs' market. At the start of the off-season, each releases its
/// fighters of `release_age` and more worth less than its average fighter, a rich club
/// at any age those worth less than `rich_release_percent` of it, worth as its
/// profile's eye sees it (`ProfileRules::recruits`), and the rookies
/// arrive. Each rich club buys a better fighter (`rich_purchases`), then each club
/// below `roster.wanted` fighters recruits (see `rival_signings`); then each club with a place,
/// keeping its `care_reserve`, offers its `buy_percent` of their value for the player's
/// listed fighters (see `sale_offers`).
pub(super) fn rival_market(state: &mut GameState, data: &DesignData) {
    let market = &data.economy.market;
    if off_season_starts(state, data) {
        for club in 0..state.rivals.len() {
            let rival = &state.rivals[club];
            let eye = rival.rules(data).recruits;
            let values: Vec<u64> = rival
                .fighters
                .iter()
                .filter(|f| f.status == Status::Active)
                .map(|f| u64::from(eye.appeal(f, market)))
                .collect();
            let count = u64::try_from(values.len()).unwrap_or(u64::MAX);
            let average = values.iter().sum::<u64>().checked_div(count).unwrap_or(0);
            let rich = rival.wealth == Wealth::Rich;
            let gone: Vec<usize> = (0..rival.fighters.len())
                .filter(|&i| {
                    let fighter = &rival.fighters[i];
                    let worth = u64::from(eye.appeal(fighter, market));
                    fighter.status == Status::Active
                        && ((fighter.age >= market.release_age && worth < average)
                            || (rich
                                && worth * 100 < average * u64::from(market.rich_release_percent)))
                })
                .collect();
            for fighter in gone {
                release_rival(state, RivalId { club, fighter });
            }
        }
        for _ in 0..market.rookies {
            let rookie = market::rookie(&mut state.rng, data);
            state.free_agents.push(rookie);
        }
    }
    // The rich buy first, so that those who sold to them recruit the same week.
    rich_purchases(state, data);
    rival_signings(state, data);
    sale_offers(state, data);
}

/// The rival clubs' offers for the player's fighters this week, from each club that
/// trades with the player, has a place and keeps its `care_reserve` after paying: for
/// a listed fighter, every such club offers its `buy_percent` of its value; for one
/// neither listed nor kept, with a chance growing with its value
/// (`MarketRules::unsolicited_percent`), one such club drawn offers its
/// `unsolicited_percent` of that. Both at its standing's `price_percent`.
fn sale_offers(state: &mut GameState, data: &DesignData) {
    let market = &data.economy.market;
    state.sale_offers.clear();
    for i in 0..state.fighters.len() {
        let fighter = &state.fighters[i];
        let listed = state.transfer_list.contains(&i);
        if fighter.status != Status::Active
            || state.kept.contains(&i)
            || !state.may_be_sold(fighter, data)
        {
            continue;
        }
        let worth = value(fighter, market);
        let offers: Vec<SaleOffer> = (0..state.rivals.len())
            .filter_map(|k| {
                let rival = &state.rivals[k];
                let profile = rival.rules(data);
                let standing = state.standing_rules(k, data);
                let mut price = percent_of(worth, profile.buy_percent);
                if !listed {
                    price = percent_of(price, profile.unsolicited_percent);
                }
                let price =
                    u32::try_from(u64::from(price) * 100 / u64::from(standing.price_percent))
                        .unwrap_or(u32::MAX);
                let pays = rival.money - i64::from(price) >= i64::from(profile.care_reserve);
                (standing.deals && state.rival_has_place(k, data) && pays).then_some(SaleOffer {
                    fighter: i,
                    rival: k,
                    price,
                })
            })
            .collect();
        if listed {
            state.sale_offers.extend(offers);
        } else if !offers.is_empty() {
            let chance = (market.unsolicited_percent + worth / market.unsolicited_value.max(1))
                .min(market.unsolicited_max_percent);
            if state.rng.random_range(0..100) < chance {
                let k = state.rng.random_range(0..offers.len());
                state.sale_offers.push(offers[k]);
            }
        }
    }
}

/// Rival fighter `id` lets go by its club: a free agent now, with its Elo and titles.
pub(super) fn release_rival(state: &mut GameState, id: RivalId) {
    let Some(fighter) = state.rivals[id.club].fighters.get_mut(id.fighter) else {
        return;
    };
    state.free_agents.push(fighter.clone());
    fighter.status = Status::Released;
    let to = Contender::FreeAgent(state.free_agents.len() - 1);
    state.transfer(Contender::Rival(id), to);
}

/// The worth to rival club `club` (its profile's eye) of its weakest fighter in
/// activity, and its index; `None` for a club without any.
pub(super) fn weakest(state: &GameState, club: usize, data: &DesignData) -> Option<(u32, usize)> {
    let market = &data.economy.market;
    let rival = &state.rivals[club];
    let eye = rival.rules(data).recruits;
    let fighters = &rival.fighters;
    (0..fighters.len())
        .filter(|&i| fighters[i].status == Status::Active)
        .map(|i| (eye.appeal(&fighters[i], market), i))
        .min()
}

/// The free agent rival club `club` would sign first: for a poor club, the one of the
/// best potential its recruiter sees (`market::estimated_potential`), the youngest of
/// equals; for the others, or a poor club whose recruiter cannot tell, the one worth
/// most to its profile's eye.
fn wanted_agent(state: &GameState, club: usize, data: &DesignData) -> Option<usize> {
    let rival = &state.rivals[club];
    let scouting = rival
        .staff
        .recruiter()
        .map(|r| r.scouting)
        .unwrap_or_default();
    let agents = &state.free_agents;
    let promising = (0..agents.len())
        .filter(|&a| agents[a].status == Status::Active)
        .filter_map(|a| {
            let potential = estimated_potential(&agents[a], &scouting)?;
            Some((potential, Reverse(agents[a].age), Reverse(a)))
        })
        .max()
        .map(|(.., Reverse(a))| a);
    match promising {
        Some(a) if rival.wealth == Wealth::Poor => Some(a),
        _ => most_valuable(agents, rival.rules(data).recruits, data),
    }
}

/// Each rival club below `roster.wanted` fighters in activity recruits until it has
/// them: the free agent it wants first (`wanted_agent`) if worth, to its eye, at least its weakest
/// fighter; if not, a club that is not poor first tries, once a week, to buy the most
/// valuable fighter of another club worth that much that it can pay at the seller's
/// `sell_percent` keeping its own `care_reserve`, if the fighter agrees
/// (`market::refuses_to_join`, against the buyer's best fame); failing that, it signs
/// the free agent anyway. A poor club signs the free agent of the best potential it
/// sees, whatever it is worth now. A club that sold a fighter this way fills its place
/// from the free agents only, so that it never buys back the fighter it sold, even when
/// its own turn had passed.
pub(super) fn rival_signings(state: &mut GameState, data: &DesignData) {
    let mut sold = vec![false; state.rivals.len()];
    for buyer in 0..state.rivals.len() {
        let tried = sold[buyer] || state.rivals[buyer].wealth == Wealth::Poor;
        sign_up(state, buyer, tried, &mut sold, data);
    }
    for club in 0..state.rivals.len() {
        if sold[club] {
            sign_up(state, club, true, &mut sold, data);
        }
    }
}

/// Rival club `buyer`'s recruiting of `rival_signings`, buying from another club unless
/// it `tried` already; marks the club it bought from in `sold`.
fn sign_up(
    state: &mut GameState,
    buyer: usize,
    mut tried: bool,
    sold: &mut [bool],
    data: &DesignData,
) {
    let market = &data.economy.market;
    let wanted = usize::try_from(data.world.roster.wanted).unwrap_or(0);
    if !state.rivals[buyer].is_open() {
        return;
    }
    while state.rivals[buyer].active() < wanted {
        let weakest = weakest(state, buyer, data).map_or(0, |(worth, _)| worth);
        let agent = wanted_agent(state, buyer, data);
        let eye = state.rivals[buyer].rules(data).recruits;
        let suits = agent.is_some_and(|a| eye.appeal(&state.free_agents[a], market) >= weakest);
        if !suits && !tried {
            tried = true;
            if let Some(seller) = buy_from_rival(state, buyer, weakest, data) {
                sold[seller] = true;
                continue;
            }
        }
        let Some(agent) = agent else { break };
        let mut signed = state.free_agents[agent].clone();
        signed.joined = Some(state.week);
        state.free_agents[agent].status = Status::Sold;
        let roster = &mut state.rivals[buyer].fighters;
        roster.push(signed);
        let to = RivalId {
            club: buyer,
            fighter: roster.len() - 1,
        };
        state.transfer(Contender::FreeAgent(agent), Contender::Rival(to));
    }
}

/// Each rich rival club, once a week, tries to buy the most valuable fighter of another
/// club worth at least `rich_upgrade_percent` of its weakest (see `buy_from_rival`);
/// past `roster.max`, it then lets its weakest go to the market.
pub(super) fn rich_purchases(state: &mut GameState, data: &DesignData) {
    let market = &data.economy.market;
    let max = usize::try_from(data.world.roster.max).unwrap_or(usize::MAX);
    for buyer in 0..state.rivals.len() {
        if state.rivals[buyer].wealth != Wealth::Rich || !state.rivals[buyer].is_open() {
            continue;
        }
        let Some((worth, _)) = weakest(state, buyer, data) else {
            continue;
        };
        let floor = percent_of(worth, market.rich_upgrade_percent);
        if buy_from_rival(state, buyer, floor, data).is_some()
            && state.rivals[buyer].active() > max
            && let Some((_, fighter)) = weakest(state, buyer, data)
        {
            release_rival(
                state,
                RivalId {
                    club: buyer,
                    fighter,
                },
            );
        }
    }
}

/// Rival club `buyer` buys the fighter of another club no richer than it worth most to
/// its eye, at least `floor`, that it can pay, if the fighter agrees; returns the seller if
/// it did. A club sells to a richer one, never to a poorer, and buys only keeping its
/// `care_reserve` and half the money of its wealth (`WealthLevel::from`), so as not to
/// fall from its level by buying.
pub(super) fn buy_from_rival(
    state: &mut GameState,
    buyer: usize,
    floor: u32,
    data: &DesignData,
) -> Option<usize> {
    let market = &data.economy.market;
    let money = state.rivals[buyer].money;
    let wealth = state.rivals[buyer].wealth;
    let level = i64::from(data.economy.wealth.get(wealth).from) / 2;
    let rules = state.rivals[buyer].rules(data);
    let reserve = i64::from(rules.care_reserve).max(level);
    let eye = rules.recruits;
    let best = state
        .rivals
        .iter()
        .enumerate()
        .filter(|&(club, rival)| club != buyer && rival.wealth <= wealth)
        .flat_map(|(club, rival)| {
            (0..rival.fighters.len()).map(move |fighter| RivalId { club, fighter })
        })
        .filter_map(|id| {
            let worth = eye.appeal(state.rival_fighter(id)?, market);
            let price = state.asking_price(id, data)?;
            (worth >= floor && money - i64::from(price) >= reserve).then_some((worth, id, price))
        })
        .max_by_key(|&(worth, id, _)| (worth, Reverse(id)));
    let (_, id, price) = best?;
    let fame = state.rivals[buyer]
        .fighters
        .iter()
        .filter(|f| f.status == Status::Active)
        .map(|f| f.fame)
        .max_by_key(|f| f.value())
        .unwrap_or_default();
    let fighter = &state.rivals[id.club].fighters[id.fighter];
    if market::refuses_to_join(&mut state.rng, fighter, fame, market) {
        return None;
    }
    let mut bought = fighter.clone();
    bought.joined = Some(state.week);
    let held: Vec<usize> = (0..state.colosseums.len())
        .filter(|&c| state.colosseums[c].champion == Some(Contender::Rival(id)))
        .collect();
    let week = state.week;
    state.news.extend(held.into_iter().map(|colosseum| News {
        week,
        what: WorldNews::ChampionBought {
            colosseum,
            fighter: id,
            buyer,
            price,
        },
    }));
    state.rivals[id.club].fighters[id.fighter].status = Status::Sold;
    state.rivals[id.club].money += i64::from(price);
    state.rivals[buyer].money -= i64::from(price);
    let roster = &mut state.rivals[buyer].fighters;
    roster.push(bought);
    let to = RivalId {
        club: buyer,
        fighter: roster.len() - 1,
    };
    state.transfer(Contender::Rival(id), Contender::Rival(to));
    Some(id.club)
}

/// Index of the one of `fighters` still active worth most to `eye`, the first of equals.
fn most_valuable(fighters: &[CareerFighter], eye: Eye, data: &DesignData) -> Option<usize> {
    (0..fighters.len())
        .filter(|&a| fighters[a].status == Status::Active)
        .max_by_key(|&a| (eye.appeal(&fighters[a], &data.economy.market), Reverse(a)))
}

/// Beyond `MarketRules::max_free_agents`, the least valuable free agents give up
/// waiting for a club: they retire.
pub(super) fn trim_market(state: &mut GameState, data: &DesignData) {
    let market = &data.economy.market;
    let most = usize::try_from(market.max_free_agents).unwrap_or(usize::MAX);
    while state.available_free_agents() > most {
        let Some(least) = (0..state.free_agents.len())
            .filter(|&a| state.free_agents[a].status == Status::Active)
            .min_by_key(|&a| (value(&state.free_agents[a], market), Reverse(a)))
        else {
            break;
        };
        state.free_agents[least].status = Status::Retired;
    }
}
