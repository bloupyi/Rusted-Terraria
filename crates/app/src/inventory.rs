//! Items, stacks and the player's inventory (the first `HOTBAR` slots are the hotbar).

use crate::items::{Material, Tool};
use world::Liquid;

pub const SLOTS: usize = 50;
pub const HOTBAR: usize = 10;
pub const MAX_STACK: u16 = 999;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    Block(u16),
    /// `None` is an empty bucket.
    Bucket(Option<Liquid>),
    /// Furniture, torches and other objects: an index into the game's templates.
    Object(u16),
    Tool(Tool),
    Material(Material),
}

impl Item {
    pub fn max_stack(self) -> u16 {
        match self {
            Item::Block(_) => MAX_STACK,
            Item::Bucket(_) | Item::Tool(_) => 1,
            Item::Material(_) => MAX_STACK,
            Item::Object(_) => 99,
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
    /// What the cursor carries (`Main.mouseItem`).
    pub mouse: Option<Stack>,
}

impl Inventory {
    pub fn new(items: &[Item]) -> Self {
        let mut inv = Self { slots: [None; SLOTS], selected: 0, mouse: None };
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

    pub fn count(&self, item: Item) -> u32 {
        self.slots.iter().flatten().filter(|s| s.item == item).map(|s| s.count as u32).sum()
    }

    /// Takes `n` of `item`, last slots first. The caller checks there are enough.
    pub fn remove(&mut self, item: Item, mut n: u16) {
        for slot in self.slots.iter_mut().rev() {
            if let Some(s) = slot.as_mut().filter(|s| s.item == item) {
                let k = n.min(s.count);
                s.count -= k;
                n -= k;
                if s.count == 0 {
                    *slot = None;
                }
            }
        }
    }

    /// The held stack: the carried one when there is one (`PickItemSelectionOverride`
    /// selects slot 58), else the selected hotbar slot.
    pub fn selected(&self) -> Option<Stack> {
        self.mouse.or(self.slots[self.selected])
    }

    fn held_mut(&mut self) -> &mut Option<Stack> {
        if self.mouse.is_some() {
            &mut self.mouse
        } else {
            &mut self.slots[self.selected]
        }
    }

    /// Uses one of the held stack.
    pub fn consume(&mut self) {
        let slot = self.held_mut();
        if let Some(s) = slot {
            s.count -= 1;
            if s.count == 0 {
                *slot = None;
            }
        }
    }

    pub fn set_selected(&mut self, item: Item) {
        *self.held_mut() = Some(Stack { item, count: 1 });
    }

    /// Takes the whole held stack out, to be thrown.
    pub fn take_held(&mut self) -> Option<Stack> {
        self.held_mut().take()
    }

    /// `ItemSlot.LeftClick` on slot `i`: swaps it with the mouse, then stacks the
    /// mouse onto it when they hold the same item. True when anything is there.
    pub fn left_click(&mut self, i: usize) -> bool {
        std::mem::swap(&mut self.slots[i], &mut self.mouse);
        if let (Some(s), Some(m)) = (&mut self.slots[i], &mut self.mouse) {
            let max = s.item.max_stack();
            if s.item == m.item && s.count != max && m.count != max {
                let n = m.count.min(max - s.count);
                s.count += n;
                m.count -= n;
            }
        }
        if self.mouse.is_some_and(|m| m.count == 0) {
            self.mouse = None;
        }
        self.mouse.is_some() || self.slots[i].is_some()
    }

    /// `ItemSlot.RightClick` on slot `i`: takes one into the mouse when it is
    /// empty or holds a smaller stack of the same item. True when one moved.
    pub fn right_click(&mut self, i: usize) -> bool {
        let Some(s) = &mut self.slots[i] else { return false };
        match &mut self.mouse {
            None => self.mouse = Some(Stack { item: s.item, count: 1 }),
            Some(m) if m.item == s.item && m.count < m.item.max_stack() => m.count += 1,
            Some(_) => return false,
        }
        s.count -= 1;
        if s.count == 0 {
            self.slots[i] = None;
        }
        true
    }

    /// Puts the mouse item back into the slots; what does not fit stays on the mouse.
    pub fn return_mouse(&mut self) {
        if let Some(m) = self.mouse.take() {
            let left = self.add(m.item, m.count);
            self.mouse = (left > 0).then_some(Stack { item: m.item, count: left });
        }
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
        assert_eq!(inv.add(Item::Block(2), 999 * SLOTS as u16), 999 * 4);
    }

    #[test]
    fn mouse_swaps_stacks_and_splits() {
        let dirt = Item::Block(0);
        let mut inv = Inventory::new(&[]);
        inv.slots[0] = Some(Stack { item: dirt, count: 990 });
        inv.slots[1] = Some(Stack { item: dirt, count: 20 });
        assert!(inv.left_click(1));
        assert_eq!((inv.slots[1], inv.mouse.map(|m| m.count)), (None, Some(20)), "picked up");
        inv.left_click(0);
        assert_eq!((inv.slots[0].unwrap().count, inv.mouse.map(|m| m.count)), (999, Some(11)), "stacked to the max");
        inv.left_click(0);
        assert_eq!((inv.slots[0].unwrap().count, inv.mouse.map(|m| m.count)), (11, Some(999)), "a full stack swaps");
        inv.slots[2] = Some(Stack { item: Item::Bucket(None), count: 1 });
        assert!(!inv.right_click(2), "the mouse holds something else");
        inv.mouse = None;
        assert!(inv.right_click(0) && inv.right_click(0));
        assert_eq!((inv.slots[0].unwrap().count, inv.mouse.map(|m| m.count)), (9, Some(2)));
        inv.return_mouse();
        assert_eq!((inv.slots[0].unwrap().count, inv.mouse), (11, None));
        assert!(!inv.left_click(5), "nothing either side");
    }

    #[test]
    fn the_carried_item_is_held_first() {
        let mut inv = Inventory::new(&[Item::Block(1)]);
        inv.add(Item::Block(1), 4);
        inv.mouse = Some(Stack { item: Item::Block(2), count: 2 });
        assert_eq!(inv.selected().map(|s| s.item), Some(Item::Block(2)));
        inv.consume();
        inv.consume();
        assert_eq!(inv.mouse, None, "used up from the mouse");
        assert_eq!(inv.selected().map(|s| s.count), Some(5), "back to the hotbar");
        assert_eq!(inv.take_held().map(|s| s.count), Some(5));
        assert_eq!(inv.slots[0], None);
    }

    #[test]
    fn count_and_remove() {
        let mut inv = Inventory::new(&[]);
        inv.add(Item::Block(1), 1200);
        assert_eq!(inv.count(Item::Block(1)), 1200);
        inv.remove(Item::Block(1), 300);
        assert_eq!(inv.count(Item::Block(1)), 900);
        assert_eq!(inv.slots[1], None, "the second stack went first");
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
