//! Items, stacks and the player's inventory (the first `HOTBAR` slots are the hotbar).

use world::Liquid;

pub const SLOTS: usize = 40;
pub const HOTBAR: usize = 10;
pub const MAX_STACK: u16 = 999;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Block(u16),
    /// `None` is an empty bucket.
    Bucket(Option<Liquid>),
}

impl Item {
    fn max_stack(self) -> u16 {
        match self {
            Item::Block(_) => MAX_STACK,
            Item::Bucket(_) => 1,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Stack {
    pub item: Item,
    pub count: u16,
}

pub struct Inventory {
    pub slots: [Option<Stack>; SLOTS],
    pub selected: usize,
}

impl Inventory {
    pub fn new(items: &[Item]) -> Self {
        let mut inv = Self { slots: [None; SLOTS], selected: 0 };
        for &item in items {
            inv.add(item, 1);
        }
        inv
    }

    /// Adds onto matching stacks first, then empty slots. Returns what did not fit.
    pub fn add(&mut self, item: Item, mut count: u16) -> u16 {
        let max = item.max_stack();
        for s in self.slots.iter_mut().flatten().filter(|s| s.item == item) {
            let n = count.min(max - s.count);
            s.count += n;
            count -= n;
        }
        for slot in self.slots.iter_mut().filter(|s| s.is_none()) {
            if count == 0 {
                break;
            }
            let n = count.min(max);
            *slot = Some(Stack { item, count: n });
            count -= n;
        }
        count
    }

    pub fn selected(&self) -> Option<Stack> {
        self.slots[self.selected]
    }

    /// Uses one of the selected stack.
    pub fn consume(&mut self) {
        let slot = &mut self.slots[self.selected];
        if let Some(s) = slot {
            s.count -= 1;
            if s.count == 0 {
                *slot = None;
            }
        }
    }

    pub fn set_selected(&mut self, item: Item) {
        self.slots[self.selected] = Some(Stack { item, count: 1 });
    }

    pub fn cycle(&mut self, step: i32) {
        self.selected = (self.selected as i32 + step).rem_euclid(HOTBAR as i32) as usize;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stacks_fill_then_spill() {
        let mut inv = Inventory::new(&[Item::Bucket(None)]);
        assert_eq!(inv.add(Item::Block(1), 1500), 0);
        assert_eq!(inv.slots[1], Some(Stack { item: Item::Block(1), count: 999 }));
        assert_eq!(inv.slots[2], Some(Stack { item: Item::Block(1), count: 501 }));
        assert_eq!(inv.add(Item::Block(1), 10), 0);
        assert_eq!(inv.slots[2].unwrap().count, 511);
        // Buckets do not stack.
        inv.add(Item::Bucket(None), 1);
        assert_eq!(inv.slots[3], Some(Stack { item: Item::Bucket(None), count: 1 }));
        // Full inventory returns the rest.
        assert_eq!(inv.add(Item::Block(2), 999 * 40), 999 * 4);
    }

    #[test]
    fn consume_empties_slot() {
        let mut inv = Inventory::new(&[]);
        inv.add(Item::Block(3), 2);
        inv.consume();
        assert_eq!(inv.selected().unwrap().count, 1);
        inv.consume();
        assert_eq!(inv.selected(), None);
        inv.cycle(-1);
        assert_eq!(inv.selected, HOTBAR - 1);
    }
}
