//! Tools, weapons, materials and crafting recipes. Item sheet numbers
//! (`Item_N`) and frame layouts follow the public item list.

use crate::inventory::Item;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tier {
    Wood,
    Copper,
    Tin,
    Iron,
    Lead,
    Silver,
    Tungsten,
    Gold,
    Platinum,
}

impl Tier {
    /// Game ids of the pickaxe, axe, hammer and broadsword (the wooden ones the
    /// game lacks are drawn as iron).
    fn ids(self) -> [u16; 4] {
        match self {
            Tier::Wood => [1, 10, 196, 24],
            Tier::Copper => [3509, 3506, 3505, 3508],
            Tier::Tin => [3503, 3500, 3499, 3502],
            Tier::Iron => [1, 10, 7, 4],
            Tier::Lead => [3497, 3494, 3493, 3496],
            Tier::Silver => [3515, 3512, 3511, 3514],
            Tier::Tungsten => [3491, 3488, 3487, 3490],
            Tier::Gold => [3521, 3518, 3517, 3520],
            Tier::Platinum => [3485, 3482, 3481, 3484],
        }
    }
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

/// Use stats of a tool or weapon, from the game's item defaults.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
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
    pub damage: i32,
    pub knock_back: f32,
    pub scale: f32,
    /// Swings again while the button stays down.
    pub auto_reuse: bool,
    /// The player may turn during the swing.
    pub use_turn: bool,
}

impl Tool {
    /// Wooden tiers that the game lacks use the copper ones.
    pub fn stats(self) -> ToolStats {
        let tool = |pick, axe, hammer, use_time, use_animation, tile_boost, damage, knock_back| ToolStats {
            pick,
            axe,
            hammer,
            use_time,
            use_animation,
            tile_boost,
            damage,
            knock_back,
            scale: 1.0,
            auto_reuse: true,
            use_turn: true,
        };
        let sword = |use_time, use_animation, damage, knock_back| ToolStats {
            use_time,
            use_animation,
            damage,
            knock_back,
            scale: 1.0,
            ..ToolStats::default()
        };
        let pick = |pick, use_time, use_animation, tile_boost, damage, scale| ToolStats {
            scale,
            ..tool(pick, 0, 0, use_time, use_animation, tile_boost, damage, 2.0)
        };
        match self {
            Tool::Pickaxe(Tier::Wood | Tier::Copper) => pick(35, 15, 23, -1, 4, 0.9),
            Tool::Pickaxe(Tier::Tin) => pick(35, 14, 21, 0, 5, 0.95),
            Tool::Pickaxe(Tier::Iron) => pick(40, 13, 20, 0, 5, 1.0),
            Tool::Pickaxe(Tier::Lead) => pick(43, 12, 19, 0, 6, 1.025),
            Tool::Pickaxe(Tier::Silver) => pick(45, 11, 19, 0, 6, 1.05),
            Tool::Pickaxe(Tier::Tungsten) => pick(50, 19, 21, 0, 6, 1.05),
            Tool::Pickaxe(Tier::Gold) => pick(55, 17, 20, 0, 6, 1.05),
            Tool::Pickaxe(Tier::Platinum) => pick(59, 15, 19, 0, 7, 1.05),
            Tool::Axe(Tier::Wood | Tier::Copper) => tool(0, 7, 0, 21, 30, -1, 3, 4.5),
            Tool::Axe(Tier::Tin) => tool(0, 8, 0, 20, 28, 0, 4, 4.5),
            Tool::Axe(Tier::Iron) => tool(0, 9, 0, 19, 27, 0, 5, 4.5),
            Tool::Axe(Tier::Lead) => tool(0, 10, 0, 19, 28, 0, 6, 4.5),
            Tool::Axe(Tier::Silver) => tool(0, 10, 0, 18, 26, 0, 6, 4.5),
            Tool::Axe(Tier::Tungsten) => tool(0, 11, 0, 18, 26, 0, 7, 4.5),
            Tool::Axe(Tier::Gold) => tool(0, 11, 0, 18, 26, 0, 7, 4.5),
            Tool::Axe(Tier::Platinum) => tool(0, 12, 0, 17, 25, 0, 8, 4.5),
            Tool::Hammer(Tier::Wood) => tool(0, 0, 25, 25, 37, -1, 2, 5.5),
            Tool::Hammer(Tier::Copper) => tool(0, 0, 35, 23, 33, -1, 4, 5.5),
            Tool::Hammer(Tier::Tin) => tool(0, 0, 38, 21, 31, 0, 6, 5.5),
            Tool::Hammer(Tier::Iron) => tool(0, 0, 40, 20, 30, 0, 7, 5.5),
            Tool::Hammer(Tier::Lead) => tool(0, 0, 43, 19, 29, 0, 8, 5.5),
            Tool::Hammer(Tier::Silver) => tool(0, 0, 45, 19, 29, 0, 9, 5.5),
            Tool::Hammer(Tier::Tungsten) => tool(0, 0, 50, 25, 28, 0, 9, 5.5),
            Tool::Hammer(Tier::Gold) => tool(0, 0, 55, 23, 28, 0, 9, 5.5),
            Tool::Hammer(Tier::Platinum) => tool(0, 0, 59, 21, 27, 0, 10, 5.5),
            Tool::Sword(Tier::Wood) => sword(20, 20, 7, 5.0),
            Tool::Sword(Tier::Copper) => sword(20, 21, 9, 5.5),
            Tool::Sword(Tier::Tin) => sword(20, 20, 10, 5.5),
            Tool::Sword(Tier::Iron) => sword(20, 20, 12, 5.5),
            Tool::Sword(Tier::Lead) => sword(20, 20, 13, 5.5),
            Tool::Sword(Tier::Silver) => sword(20, 20, 14, 6.0),
            Tool::Sword(Tier::Tungsten) => sword(19, 19, 14, 6.0),
            Tool::Sword(Tier::Gold) => sword(18, 18, 15, 6.5),
            Tool::Sword(Tier::Platinum) => sword(17, 17, 16, 6.5),
        }
    }

    /// The game's id of the item.
    pub fn id(self) -> u16 {
        match self {
            Tool::Pickaxe(t) => t.ids()[0],
            Tool::Axe(t) => t.ids()[1],
            Tool::Hammer(t) => t.ids()[2],
            Tool::Sword(t) => t.ids()[3],
        }
    }

    pub fn sheet(self) -> String {
        format!("Item_{}", self.id())
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
        out: Out::Item(Item::Id(crate::magic::MANA_CRYSTAL)),
        count: 1,
        inputs: &[(Item::Id(crate::magic::FALLEN_STAR), 5)],
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
    Recipe { out: Out::Item(Item::Id(703)), count: 1, inputs: &[(Item::Block(166), 3)], station: Some(FURNACE) },
    Recipe { out: Out::Item(Item::Id(21)), count: 1, inputs: &[(Item::Block(9), 4)], station: Some(FURNACE) },
    Recipe { out: Out::Item(Item::Id(705)), count: 1, inputs: &[(Item::Block(168), 4)], station: Some(FURNACE) },
    Recipe { out: Out::Item(Item::Id(19)), count: 1, inputs: &[(Item::Block(8), 4)], station: Some(FURNACE) },
    Recipe { out: Out::Item(Item::Id(706)), count: 1, inputs: &[(Item::Block(169), 4)], station: Some(FURNACE) },
    Recipe {
        out: Out::Furniture(Furniture::Anvil),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 5)],
        station: Some(WORK_BENCH),
    },
    Recipe {
        out: Out::Item(Item::Id(crate::magic::AMETHYST_STAFF)),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 7), (Item::Id(181), 8)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Id(740)),
        count: 1,
        inputs: &[(Item::Id(703), 7), (Item::Id(180), 8)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Id(741)),
        count: 1,
        inputs: &[(Item::Id(21), 7), (Item::Id(177), 8)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Id(742)),
        count: 1,
        inputs: &[(Item::Id(705), 7), (Item::Id(179), 8)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Id(743)),
        count: 1,
        inputs: &[(Item::Id(19), 7), (Item::Id(178), 8)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Id(744)),
        count: 1,
        inputs: &[(Item::Id(706), 7), (Item::Id(182), 8)],
        station: Some(ANVIL),
    },
    // Pickaxe, axe, hammer and broadsword of each metal.
    Recipe {
        out: Out::Item(Item::Tool(Tool::Pickaxe(Tier::Copper))),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 8), wood(4)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Axe(Tier::Copper))),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 6), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Hammer(Tier::Copper))),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Sword(Tier::Copper))),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 6)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Pickaxe(Tier::Tin))),
        count: 1,
        inputs: &[(Item::Id(703), 8), wood(4)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Axe(Tier::Tin))),
        count: 1,
        inputs: &[(Item::Id(703), 6), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Hammer(Tier::Tin))),
        count: 1,
        inputs: &[(Item::Id(703), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Sword(Tier::Tin))),
        count: 1,
        inputs: &[(Item::Id(703), 6)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Pickaxe(Tier::Iron))),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 10), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Axe(Tier::Iron))),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Hammer(Tier::Iron))),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Sword(Tier::Iron))),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 8)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Pickaxe(Tier::Lead))),
        count: 1,
        inputs: &[(Item::Id(704), 10), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Axe(Tier::Lead))),
        count: 1,
        inputs: &[(Item::Id(704), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Hammer(Tier::Lead))),
        count: 1,
        inputs: &[(Item::Id(704), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Sword(Tier::Lead))),
        count: 1,
        inputs: &[(Item::Id(704), 8)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Pickaxe(Tier::Silver))),
        count: 1,
        inputs: &[(Item::Id(21), 10), wood(4)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Axe(Tier::Silver))),
        count: 1,
        inputs: &[(Item::Id(21), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Hammer(Tier::Silver))),
        count: 1,
        inputs: &[(Item::Id(21), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Sword(Tier::Silver))),
        count: 1,
        inputs: &[(Item::Id(21), 8)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Pickaxe(Tier::Tungsten))),
        count: 1,
        inputs: &[(Item::Id(705), 10), wood(4)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Axe(Tier::Tungsten))),
        count: 1,
        inputs: &[(Item::Id(705), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Hammer(Tier::Tungsten))),
        count: 1,
        inputs: &[(Item::Id(705), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Sword(Tier::Tungsten))),
        count: 1,
        inputs: &[(Item::Id(705), 8)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Pickaxe(Tier::Gold))),
        count: 1,
        inputs: &[(Item::Id(19), 10), wood(4)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Axe(Tier::Gold))),
        count: 1,
        inputs: &[(Item::Id(19), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Hammer(Tier::Gold))),
        count: 1,
        inputs: &[(Item::Id(19), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Sword(Tier::Gold))),
        count: 1,
        inputs: &[(Item::Id(19), 8)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Pickaxe(Tier::Platinum))),
        count: 1,
        inputs: &[(Item::Id(706), 10), wood(4)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Axe(Tier::Platinum))),
        count: 1,
        inputs: &[(Item::Id(706), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Hammer(Tier::Platinum))),
        count: 1,
        inputs: &[(Item::Id(706), 8), wood(3)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Tool(Tool::Sword(Tier::Platinum))),
        count: 1,
        inputs: &[(Item::Id(706), 8)],
        station: Some(ANVIL),
    },
    // Bows and wooden arrows.
    Recipe { out: Out::Item(Item::Id(39)), count: 1, inputs: &[wood(10)], station: Some(WORK_BENCH) },
    Recipe {
        out: Out::Item(Item::Id(40)),
        count: 25,
        inputs: &[wood(1), (Item::Block(STONE), 1)],
        station: Some(WORK_BENCH),
    },
    Recipe {
        out: Out::Item(Item::Id(3504)),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 7)],
        station: Some(ANVIL),
    },
    Recipe { out: Out::Item(Item::Id(3498)), count: 1, inputs: &[(Item::Id(703), 7)], station: Some(ANVIL) },
    Recipe {
        out: Out::Item(Item::Id(99)),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 7)],
        station: Some(ANVIL),
    },
    Recipe { out: Out::Item(Item::Id(3492)), count: 1, inputs: &[(Item::Id(704), 7)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(3510)), count: 1, inputs: &[(Item::Id(21), 7)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(3486)), count: 1, inputs: &[(Item::Id(705), 7)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(3516)), count: 1, inputs: &[(Item::Id(19), 7)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(3480)), count: 1, inputs: &[(Item::Id(706), 7)], station: Some(ANVIL) },
    // Armour: head, body, legs of each set.
    Recipe { out: Out::Item(Item::Id(704)), count: 1, inputs: &[(Item::Block(167), 3)], station: Some(FURNACE) },
    Recipe { out: Out::Item(Item::Id(727)), count: 1, inputs: &[wood(20)], station: Some(WORK_BENCH) },
    Recipe { out: Out::Item(Item::Id(728)), count: 1, inputs: &[wood(30)], station: Some(WORK_BENCH) },
    Recipe { out: Out::Item(Item::Id(729)), count: 1, inputs: &[wood(25)], station: Some(WORK_BENCH) },
    Recipe {
        out: Out::Item(Item::Id(89)),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 12)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Id(80)),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 20)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Id(76)),
        count: 1,
        inputs: &[(Item::Material(Material::CopperBar), 16)],
        station: Some(ANVIL),
    },
    Recipe { out: Out::Item(Item::Id(687)), count: 1, inputs: &[(Item::Id(703), 12)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(688)), count: 1, inputs: &[(Item::Id(703), 20)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(689)), count: 1, inputs: &[(Item::Id(703), 16)], station: Some(ANVIL) },
    Recipe {
        out: Out::Item(Item::Id(90)),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 15)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Id(81)),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 25)],
        station: Some(ANVIL),
    },
    Recipe {
        out: Out::Item(Item::Id(77)),
        count: 1,
        inputs: &[(Item::Material(Material::IronBar), 20)],
        station: Some(ANVIL),
    },
    Recipe { out: Out::Item(Item::Id(690)), count: 1, inputs: &[(Item::Id(704), 15)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(691)), count: 1, inputs: &[(Item::Id(704), 25)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(692)), count: 1, inputs: &[(Item::Id(704), 20)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(91)), count: 1, inputs: &[(Item::Id(21), 15)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(82)), count: 1, inputs: &[(Item::Id(21), 25)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(78)), count: 1, inputs: &[(Item::Id(21), 20)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(693)), count: 1, inputs: &[(Item::Id(705), 15)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(694)), count: 1, inputs: &[(Item::Id(705), 25)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(695)), count: 1, inputs: &[(Item::Id(705), 20)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(92)), count: 1, inputs: &[(Item::Id(19), 20)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(83)), count: 1, inputs: &[(Item::Id(19), 30)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(79)), count: 1, inputs: &[(Item::Id(19), 25)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(696)), count: 1, inputs: &[(Item::Id(706), 20)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(697)), count: 1, inputs: &[(Item::Id(706), 30)], station: Some(ANVIL) },
    Recipe { out: Out::Item(Item::Id(698)), count: 1, inputs: &[(Item::Id(706), 25)], station: Some(ANVIL) },
];

/// The pictures of every tier's tools, as `Tier::ids` lists them.
const TOOL_SHEETS: &[&str] = &[
    "Item_1",
    "Item_10",
    "Item_196",
    "Item_24",
    "Item_3509",
    "Item_3506",
    "Item_3505",
    "Item_3508",
    "Item_3503",
    "Item_3500",
    "Item_3499",
    "Item_3502",
    "Item_7",
    "Item_4",
    "Item_3497",
    "Item_3494",
    "Item_3493",
    "Item_3496",
    "Item_3515",
    "Item_3512",
    "Item_3511",
    "Item_3514",
    "Item_3491",
    "Item_3488",
    "Item_3487",
    "Item_3490",
    "Item_3521",
    "Item_3518",
    "Item_3517",
    "Item_3520",
    "Item_3485",
    "Item_3482",
    "Item_3481",
    "Item_3484",
];

/// Every sheet the items above draw with.
pub fn sheets() -> Vec<String> {
    let mut v: Vec<String> =
        [Material::Gel, Material::CopperBar, Material::IronBar, Material::Lens, Material::SuspiciousEye]
            .iter()
            .map(|m| m.sheet().to_string())
            .collect();
    v.extend(TOOL_SHEETS.iter().map(|s| s.to_string()));
    v.push("Item_9".to_string());
    v.sort();
    v.dedup();
    v
}
