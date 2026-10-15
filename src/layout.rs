//! Gives every building a lot that it keeps for its whole life.
//!
//! The city is a grid of square blocks of `BLOCK_SIDE`×`BLOCK_SIDE` lots with
//! a one-tile road between blocks. Blocks are handed out along a square spiral
//! around the origin, so the city grows outwards from the centre. Each block
//! belongs to one namespace; a namespace that outgrows its blocks gets another.

use std::collections::HashSet;

pub const BLOCK_SIDE: i32 = 4;
/// Lots plus the road that follows them.
pub const BLOCK_STRIDE: i32 = BLOCK_SIDE + 1;
const LOTS_PER_BLOCK: usize = (BLOCK_SIDE * BLOCK_SIDE) as usize;

/// A building's place: a block (by spiral index) and a lot inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lot {
    pub block: usize,
    pub index: usize,
}

#[derive(Debug)]
pub struct Block {
    pub namespace: String,
    taken: [bool; LOTS_PER_BLOCK],
}

impl Block {
    fn with_first_lot(namespace: &str) -> Self {
        let mut taken = [false; LOTS_PER_BLOCK];
        taken[0] = true;
        Self { namespace: namespace.to_owned(), taken }
    }

    /// Takes the lowest free lot, so lots fill from the back of the block.
    fn take(&mut self) -> Option<usize> {
        let index = self.taken.iter().position(|taken| !taken)?;
        self.taken[index] = true;
        Some(index)
    }

    fn is_empty(&self) -> bool {
        !self.taken.contains(&true)
    }
}

#[derive(Debug, Default)]
pub struct Layout {
    blocks: Vec<Option<Block>>,
}

impl Layout {
    pub fn place(&mut self, namespace: &str) -> Lot {
        let in_own_block = self.blocks.iter_mut().enumerate().find_map(|(block, slot)| {
            let owned = slot.as_mut().filter(|b| b.namespace == namespace)?;
            Some(Lot { block, index: owned.take()? })
        });
        if let Some(lot) = in_own_block {
            return lot;
        }
        let block = match self.blocks.iter().position(Option::is_none) {
            Some(free) => free,
            None => {
                self.blocks.push(None);
                self.blocks.len() - 1
            }
        };
        self.blocks[block] = Some(Block::with_first_lot(namespace));
        Lot { block, index: 0 }
    }

    /// Frees the lot, and its block once nothing stands on it any more.
    pub fn release(&mut self, lot: Lot) {
        let Some(slot) = self.blocks.get_mut(lot.block) else {
            return;
        };
        let now_empty = slot.as_mut().is_some_and(|block| {
            block.taken[lot.index] = false;
            block.is_empty()
        });
        if now_empty {
            *slot = None;
        }
    }

    /// The tile a lot sits on, in world tile coordinates.
    pub fn tile(lot: Lot) -> (i32, i32) {
        let (bx, by) = spiral(lot.block);
        let index = lot.index as i32;
        (bx * BLOCK_STRIDE + index % BLOCK_SIDE, by * BLOCK_STRIDE + index / BLOCK_SIDE)
    }

    /// The block a tile belongs to (roads included), if a namespace owns it.
    pub fn block_at(&self, tx: i32, ty: i32) -> Option<&Block> {
        let index = spiral_index(tx.div_euclid(BLOCK_STRIDE), ty.div_euclid(BLOCK_STRIDE));
        self.blocks.get(index)?.as_ref()
    }

    /// How many spiral rings the occupied blocks reach out to (0 = centre only).
    pub fn ring(&self) -> i32 {
        self.blocks
            .iter()
            .enumerate()
            .filter(|(_, block)| block.is_some())
            .map(|(index, _)| {
                let (x, y) = spiral(index);
                x.abs().max(y.abs())
            })
            .max()
            .unwrap_or(0)
    }

    /// Each namespace once, with its first (lowest-index) block.
    pub fn districts(&self) -> impl Iterator<Item = (usize, &str)> {
        let mut seen = HashSet::new();
        self.blocks.iter().enumerate().filter_map(move |(index, block)| {
            let namespace = block.as_ref()?.namespace.as_str();
            seen.insert(namespace).then_some((index, namespace))
        })
    }
}

pub fn is_road(tx: i32, ty: i32) -> bool {
    tx.rem_euclid(BLOCK_STRIDE) == BLOCK_SIDE || ty.rem_euclid(BLOCK_STRIDE) == BLOCK_SIDE
}

/// The n-th position of a square spiral around (0, 0): ring `k` holds the
/// `8k` positions with `max(|x|, |y|) == k`.
pub fn spiral(n: usize) -> (i32, i32) {
    if n == 0 {
        return (0, 0);
    }
    let mut k: i64 = 1;
    while ((2 * k + 1) * (2 * k + 1)) as usize <= n {
        k += 1;
    }
    let m = n as i64 - (2 * k - 1) * (2 * k - 1);
    let (side, offset) = (m / (2 * k), m % (2 * k));
    let (x, y) = match side {
        0 => (k, -k + 1 + offset),
        1 => (k - 1 - offset, k),
        2 => (-k, k - 1 - offset),
        _ => (-k + 1 + offset, -k),
    };
    (x as i32, y as i32)
}

/// The inverse of [`spiral`].
pub fn spiral_index(x: i32, y: i32) -> usize {
    let (x, y) = (i64::from(x), i64::from(y));
    let k = x.abs().max(y.abs());
    if k == 0 {
        return 0;
    }
    let (side, offset) = if x == k && y > -k {
        (0, y + k - 1)
    } else if y == k {
        (1, k - 1 - x)
    } else if x == -k {
        (2, k - 1 - y)
    } else {
        (3, x + k - 1)
    };
    ((2 * k - 1) * (2 * k - 1) + side * 2 * k + offset) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spiral_walks_rings_outwards_and_inverts() {
        let mut seen = HashSet::new();
        for n in 0..2000 {
            let (x, y) = spiral(n);
            assert!(seen.insert((x, y)), "position {n} repeats ({x}, {y})");
            assert_eq!(spiral_index(x, y), n);
        }
        let ring_one: HashSet<_> = (1..9).map(spiral).collect();
        assert!(ring_one.iter().all(|&(x, y)| x.abs().max(y.abs()) == 1));
    }

    #[test]
    fn each_namespace_gets_its_own_block() {
        let mut layout = Layout::default();
        let a = layout.place("a");
        let b = layout.place("b");
        assert_ne!(a.block, b.block);
        assert_eq!(layout.place("a").block, a.block);
    }

    #[test]
    fn a_full_block_spills_into_a_new_one() {
        let mut layout = Layout::default();
        let lots: Vec<_> = (0..=LOTS_PER_BLOCK).map(|_| layout.place("busy")).collect();
        assert!(lots[..LOTS_PER_BLOCK].iter().all(|lot| lot.block == 0));
        assert_eq!(lots[LOTS_PER_BLOCK], Lot { block: 1, index: 0 });
        assert_eq!(layout.districts().count(), 1);
    }

    #[test]
    fn neighbours_stay_put_when_a_building_leaves() {
        let mut layout = Layout::default();
        let lots: Vec<_> = (0..5).map(|_| layout.place("a")).collect();
        layout.release(lots[1]);
        let tiles: Vec<_> = [0, 2, 3, 4].iter().map(|&i| Layout::tile(lots[i])).collect();
        layout.place("b");
        let after: Vec<_> = [0, 2, 3, 4].iter().map(|&i| Layout::tile(lots[i])).collect();
        assert_eq!(tiles, after);
        assert_eq!(layout.place("a"), lots[1], "the freed lot is reused first");
    }

    #[test]
    fn an_empty_block_goes_back_to_the_city() {
        let mut layout = Layout::default();
        let a = layout.place("a");
        layout.place("b");
        layout.release(a);
        assert_eq!(layout.districts().map(|(_, ns)| ns).collect::<Vec<_>>(), ["b"]);
        assert_eq!(layout.place("c").block, a.block);
    }

    #[test]
    fn roads_run_between_blocks() {
        assert!(is_road(BLOCK_SIDE, 0));
        assert!(is_road(0, -1));
        assert!(!is_road(0, 0));
        assert!(!is_road(BLOCK_SIDE - 1, BLOCK_SIDE - 1));
    }
}
