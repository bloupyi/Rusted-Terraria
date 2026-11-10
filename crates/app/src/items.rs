//! Tools, weapons, materials and crafting recipes. Item sheet numbers
//! (`Item_N`) and frame layouts follow the public item list.

use crate::inventory::Item;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    Wood,
    Copper,
    Iron,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Pickaxe(Tier),
    Axe(Tier),
    Hammer(Tier),
    Sword(Tier),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Material {
    Gel,
    CopperBar,
    IronBar,
    Lens,
    /// Calls the eye boss at night.
    SuspiciousEye,
}

/// Placeable objects a recipe can make, built as fresh templates.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Furniture {
    WorkBench,
    Furnace,
    Anvil,
    Torch,
}

impl Furniture {
    /// Tile type and size in tiles; frames are 18 px apart from (0, 0).
    pub fn tile(self) -> (u16, i32, i32) {
        match self {
            Furniture::WorkBench => (WORK_BENCH, 2, 1),
            Furniture::Furnace => (FURNACE, 3, 2),
            Furniture::Anvil => (ANVIL, 2, 1),
            Furniture::Torch => (4, 1, 1),
        }
    }
}

pub const WORK_BENCH: u16 = 18;
pub const FURNACE: u16 = 17;
pub const ANVIL: u16 = 16;
pub const WOOD: u16 = 30;
pub const STONE: u16 = 1;
pub const COPPER_ORE: u16 = 7;
pub const IRON_ORE: u16 = 6;
pub const DEMONITE_ORE: u16 = 22;

/// Mining stats of a tool, from the game's item defaults.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct ToolStats {
    pub pick: i32,
    pub axe: i32,
    pub hammer: i32,
    /// Ticks between two hits.
    pub use_time: i32,
    /// Ticks of one swing.
    pub use_animation: i32,
    /// Extra reach in tiles.
    pub tile_boost: i32,
}

impl Tool {
    /// Wooden tiers that the game lacks use the copper ones.
    pub fn stats(self) -> ToolStats {
        let s = |pick, axe, hammer, use_time, use_animation, tile_boost| ToolStats {
            pick,
            axe,
            hammer,
            use_time,
            use_animation,
            tile_boost,
        };
        match self {
            Tool::Pickaxe(Tier::Iron) => s(40, 0, 0, 13, 20, 0),
            Tool::Pickaxe(_) => s(35, 0, 0, 15, 23, -1),
            Tool::Axe(Tier::Iron) => s(0, 9, 0, 19, 27, 0),
            Tool::Axe(_) => s(0, 7, 0, 21, 30, -1),
            Tool::Hammer(Tier::Wood) => s(0, 0, 25, 25, 37, -1),
            Tool::Hammer(Tier::Copper) => s(0, 0, 35, 23, 33, -1),
            Tool::Hammer(Tier::Iron) => s(0, 0, 40, 20, 30, 0),
            Tool::Sword(_) => ToolStats::default(),
        }
    }

    pub fn damage(self) -> i32 {
        match self {
            Tool::Sword(Tier::Wood) => 7,
            Tool::Sword(Tier::Copper) => 10,
            Tool::Sword(Tier::Iron) => 13,
            Tool::Pickaxe(_) | Tool::Axe(_) | Tool::Hammer(_) => 5,
        }
    }

    pub fn sheet(self) -> &'static str {
        match self {
            Tool::Pickaxe(Tier::Copper) => "Item_3509",
            Tool::Pickaxe(_) => "Item_1",
            Tool::Axe(Tier::Copper) => "Item_3506",
            Tool::Axe(_) => "Item_10",
            Tool::Hammer(Tier::Wood) => "Item_196",
            Tool::Hammer(Tier::Copper) => "Item_3505",
            Tool::Hammer(Tier::Iron) => "Item_7",
            Tool::Sword(Tier::Wood) => "Item_24",
            Tool::Sword(Tier::Copper) => "Item_3508",
            Tool::Sword(Tier::Iron) => "Item_4",
        }
    }
}

impl Material {
    pub fn sheet(self) -> &'static str {
        match self {
            Material::Gel => "Item_23",
            Material::CopperBar => "Item_20",
            Material::IronBar => "Item_22",
            Material::Lens => "Item_38",
            Material::SuspiciousEye => "Item_43",
        }
    }
}

/// Stone, ores and obsidian ring instead of thudding when hit.
pub const HARD_SOUND: [u16; 7] = [1, 6, 7, 8, 9, 25, 56];

pub struct Recipe {
    pub out: Out,
    pub count: u16,
    pub inputs: &'static [(Item, u16)],
    pub station: Option<u16>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Out {
    Item(Item),
    Furniture(Furniture),
}

const fn wood(n: u16) -> (Item, u16) {
    (Item::Block(WOOD), n)
}

pub const RECIPES: &[Recipe] = &[
    Recipe { out: Out::Furniture(Furniture::WorkBench), count: 1, inputs: &[wood(10)], station: None },
    Recipe {
        out: Out::Furniture(Furniture::Torch),
        count: 3,
        inputs: &[wood(1), (Item::Material(Material::Gel), 1)],
        station: None,
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Sword(Tier::Wood))),
        count: 1,
        inputs: &[wood(7)],
        station: Some(WORK_BENCH),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Hammer(Tier::Wood))),
        count: 1,
        inputs: &[wood(8)],
        station: Some(WORK_BENCH),
    },
    Recipe {
        out: Out::Furniture(Furniture::Furnace),
        count: 1,
        inputs: &[(Item::Block(STONE), 20), wood(4)],
        station: Some(WORK_BENCH),
    },
    Recipe {
        out: Out::Item(Item::Material(Material::SuspiciousEye)),
        count: 1,
        inputs: &[(Item::Material(Material::Lens), 6)],
        station: Some(WORK_BENCH),
    },
    Recipe {
        out: Out::Item(Item::Material(Material::CopperBar)),
        count: 1,
        inputs: &[(Item::Block(COPPER_ORE), 3)],
        station: Some(FURNACE),
    },
    Recipe {
        out: Out::Item(Item::Material(Material::IronBar)),
        count: 1,
        inputs: &[(Item::Block(IRON_ORE), 3)],
        station: Some(FURNACE),
    },
    Recipe {
        out: Out::Furniture(Furniture::Anvil),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 5)],
        station: Some(WORK_BENCH),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Pickaxe(Tier::Copper))),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 12), wood(4)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Axe(Tier::Copper))),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 9), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Hammer(Tier::Copper))),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 10), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Sword(Tier::Copper))),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 8)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Pickaxe(Tier::Iron))),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 12), wood(4)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Axe(Tier::Iron))),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 9), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Hammer(Tier::Iron))),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 10), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Sword(Tier::Iron))),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 8)],
        station: Some(ANVIL),
    },
];

/// Every sheet the items above draw with.
pub fn sheets() -> Vec<String> {
    let mut v: Vec<String> =
        [Material::Gel, Material::CopperBar, Material::IronBar, Material::Lens, Material::SuspiciousEye]
            .iter()
            .map(|m| m.sheet().to_string())
            .collect();
    for t in [Tier::Wood, Tier::Copper, Tier::Iron] {
        for tool in [Tool::Pickaxe(t), Tool::Axe(t), Tool::Hammer(t), Tool::Sword(t)] {
            v.push(tool.sheet().to_string());
        }
    }
    v.push("Item_9".to_string());
    v.sort();
    v.dedup();
    v
}
