//! Cramming: practice on the cards a list is showing, in its order, due or not. Nothing is recorded and no schedule
//! changes, so this is only a queue the client keeps: a forgotten card goes back to the end, and the cram ends when
//! every card has been remembered once.

use std::collections::VecDeque;

use crate::CardView;

#[derive(Clone, Debug, Default)]
pub struct Cram {
    queue: VecDeque<CardView>,
    remembered: usize,
}

impl Cram {
    pub fn new(cards: impl IntoIterator<Item = CardView>) -> Cram {
        Cram { queue: cards.into_iter().collect(), remembered: 0 }
    }

    /// The card to show, or `None` once every card has been remembered.
    pub fn current(&self) -> Option<&CardView> {
        self.queue.front()
    }

    /// Answers the current card: remembered, it's done; forgotten, it goes to the end.
    pub fn answer(&mut self, remembered: bool) {
        let Some(card) = self.queue.pop_front() else { return };
        if remembered {
            self.remembered += 1;
        } else {
            self.queue.push_back(card);
        }
    }

    /// Cards still to remember, the current one included.
    pub fn left(&self) -> usize {
        self.queue.len()
    }

    /// Cards remembered so far.
    pub fn remembered(&self) -> usize {
        self.remembered
    }

    pub fn finished(&self) -> bool {
        self.queue.is_empty()
    }

    /// Takes in an edit made to a card while it waits.
    pub fn update(&mut self, card: CardView) {
        if let Some(c) = self.queue.iter_mut().find(|c| c.card.id == card.card.id) {
            *c = card;
        }
    }

    /// Drops a card that was deleted.
    pub fn remove(&mut self, id: &str) {
        self.queue.retain(|c| c.card.id != id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Card;

    fn card(id: &str) -> CardView {
        let card = Card {
            id: id.into(),
            deck_id: "deck".into(),
            content: id.into(),
            tags: Vec::new(),
            created_at: 0,
            updated_at: 0,
            deleted: false,
        };
        CardView::new(card, furiten_srs::Schedule::new(0).into())
    }

    fn ids(c: &Cram) -> Vec<String> {
        c.queue.iter().map(|c| c.card.id.clone()).collect()
    }

    #[test]
    fn forgotten_cards_come_round_until_remembered() {
        let mut c = Cram::new(["a", "b", "c"].map(card));
        assert_eq!(c.current().map(|c| c.card.id.as_str()), Some("a"));
        c.answer(false);
        assert_eq!(ids(&c), ["b", "c", "a"]);
        c.answer(true);
        c.answer(false);
        assert_eq!(ids(&c), ["a", "c"]);
        assert_eq!((c.left(), c.remembered()), (2, 1));
        c.answer(true);
        c.answer(true);
        assert!(c.finished() && c.current().is_none());
        assert_eq!(c.remembered(), 3);
        c.answer(true);
        assert_eq!(c.remembered(), 3, "nothing left to answer");
    }

    #[test]
    fn edits_and_deletes_reach_the_queue() {
        let mut c = Cram::new(["a", "b"].map(card));
        let mut edited = card("b");
        edited.sides = vec!["new".into()];
        c.update(edited);
        c.remove("a");
        assert_eq!(c.current().map(|c| c.sides[0].as_str()), Some("new"));
        assert_eq!(c.left(), 1);
    }
}
