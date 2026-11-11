#!/usr/bin/env python3
"""Writes crates/app/src/item_ids.rs from the local decompiled sources.

Only numbers and internal identifiers come out: which item places each tile
(style 0), and for the items the engine uses their `ItemID` name, which is the
key of their localized name and tooltip, with the defaults the tooltip shows.
No text, no code.
Usage: tools/gen_item_names.py .local/terraria/<id>/sources
"""

import re
import sys
from pathlib import Path

CASE_RE = re.compile(r"^\t\tcase (\d+):$")
CREATE_RE = re.compile(r"\bcreateTile = (\d+);")
STYLE_RE = re.compile(r"\bplaceStyle = (\d+);")
PLACE_RE = re.compile(r"DefaultToPlaceableTile\(\(ushort\)(\d+)(?:, (\d+))?\)")
ID_RE = re.compile(r"public const short (\w+) = (\d+);")
USED_RE = re.compile(r'"Item_(\d+)"')
ASSIGN_RE = re.compile(r"^\t\t\t(\w+) = (.+);$")
BASE_RE = re.compile(r"^\t\t\tSetDefaults[1-5]\((\d+)\);$")
INGREDIENT_RE = re.compile(r"requiredItem\[\d+\]\.SetDefaults\((\d+)\);")
GROUP_RE = re.compile(r"RecipeGroups\.(\w+) = new RecipeGroup\([^,]+, ([\d, ]+)\)")
NEEDS_GROUP_RE = re.compile(r"RequireGroup\(RecipeGroups\.(\w+)\)")
END = ("\t\t\tbreak;", "\t\t\treturn;")
FLAGS = ["melee", "ranged", "magic", "summon", "consumable", "material", "ammo", "equip", "places", "container", "potion"]


def item_defaults(item_cs):
    """Assignments of each item's `SetDefaultsN` case, bases first, as `ResetStats` leaves the rest."""
    text = item_cs[item_cs.index("private void SetDefaults1(int type)"):]
    bodies, labels, body = {}, [], None
    for line in text.split("\n"):
        m = CASE_RE.match(line)
        if m:
            if body is not None:
                labels, body = [], None
            labels.append(int(m.group(1)))
            continue
        if not labels:
            continue
        if body is None:
            body = []
            for i in labels:
                bodies.setdefault(i, body)
        if line in END:
            labels, body = [], None
            continue
        body.append(line)

    def fields(i, depth=0):
        out = {"createTile": "-1"}
        for line in bodies.get(i, []):
            if (m := BASE_RE.match(line)) and depth < 4:
                out.update(fields(int(m.group(1)), depth + 1))
            elif (m := ASSIGN_RE.match(line)):
                out[m.group(1)] = m.group(2)
            elif (m := PLACE_RE.search(line)):
                out["createTile"], out["consumable"] = m.group(1), "true"
            elif "DefaultToPlaceableWall(" in line:
                out["createWall"], out["consumable"] = "1", "true"
        return out
    return fields


def materials(recipe_cs):
    """`UpdateWhichItemsAreMaterials`: every ingredient, and every item of a required group."""
    groups = {n: [int(v) for v in ids.split(",")] for n, ids in GROUP_RE.findall(recipe_cs)}
    out = {int(i) for i in INGREDIENT_RE.findall(recipe_cs)}
    for g in set(NEEDS_GROUP_RE.findall(recipe_cs)):
        out |= set(groups.get(g, []))
    return out


def num(f, key, default=0):
    v = f.get(key)
    if v is None:
        return default
    m = re.fullmatch(r"(-?\d+(?:\.\d+)?)f?", v.strip())
    return float(m.group(1)) if m else default


def max_stack(f):
    """`maxStack`; this decompilation sets none for tools, weapons and armour,
    which the game keeps one per slot."""
    if "maxStack" in f:
        return int(num(f, "maxStack", 9999))
    single = any(num(f, k) > 0 for k in ("damage", "pick", "axe", "hammer", "headSlot", "bodySlot", "legSlot"))
    single |= f.get("accessory") == "true"
    if single and f.get("consumable") != "true" and f.get("ammo", "0") == "0":
        return 1
    return 9999


def stats(f, material):
    flag = {
        "melee": f.get("melee") == "true",
        "ranged": f.get("ranged") == "true",
        "magic": f.get("magic") == "true",
        "summon": f.get("summon") == "true",
        "consumable": f.get("consumable") == "true",
        "material": material and f.get("material") != "false",
        "ammo": f.get("ammo", "0") != "0" and f.get("notAmmo") != "true",
        "equip": any(num(f, k) > 0 for k in ("headSlot", "bodySlot", "legSlot")) or f.get("accessory") == "true",
        "places": num(f, "createTile", -1) > -1 or num(f, "createWall") > 0,
        "container": num(f, "createTile", -1) in (21, 467),
        "potion": f.get("potion") == "true",
    }
    bits = " | ".join(f"{k.upper()}" for k in FLAGS if flag[k]) or "0"
    kb = float(num(f, "knockBack"))
    return (f"rare: {int(num(f, 'rare'))}, damage: {int(num(f, 'damage'))}, crit: {int(num(f, 'crit'))}, "
            f"knock_back: {kb!r}, use_style: {int(num(f, 'useStyle'))}, use_animation: {int(num(f, 'useAnimation'))}, "
            f"pick: {int(num(f, 'pick'))}, axe: {int(num(f, 'axe'))}, hammer: {int(num(f, 'hammer'))}, "
            f"tile_boost: {int(num(f, 'tileBoost'))}, defense: {int(num(f, 'defense'))}, "
            f"heal_life: {int(num(f, 'healLife'))}, mana: {int(num(f, 'mana'))}, "
            f"shoot_speed: {float(num(f, 'shootSpeed'))!r}, max_stack: {max_stack(f)}, "
            f"head_slot: {int(num(f, 'headSlot'))}, body_slot: {int(num(f, 'bodySlot'))}, "
            f"leg_slot: {int(num(f, 'legSlot'))}, flags: {bits}")


def placing_items(item_cs):
    """(item id, tile, style) for every `case` of the SetDefaults switches."""
    out = []
    labels, body, tile, style = [], False, None, 0
    for line in item_cs.split("\n"):
        m = CASE_RE.match(line)
        if m:
            if body:
                labels, body, tile, style = [], False, None, 0
            labels.append(int(m.group(1)))
            continue
        if not labels:
            continue
        body = True
        if (m := CREATE_RE.search(line)):
            tile = int(m.group(1))
        if (m := STYLE_RE.search(line)):
            style = int(m.group(1))
        if (m := PLACE_RE.search(line)):
            tile, style = int(m.group(1)), int(m.group(2) or 0)
        if line == "\t\t\tbreak;":
            if tile is not None:
                out += [(i, tile, style) for i in labels]
            labels, body, tile, style = [], False, None, 0
    return out


def main():
    src = Path(sys.argv[1])
    root = Path(__file__).resolve().parent.parent
    names = {int(i): n for n, i in ID_RE.findall((src / "Terraria.ID/ItemID.cs").read_text())}
    item_cs = (src / "Terraria/Item.cs").read_text()
    defaults = item_defaults(item_cs)
    material = materials((src / "Terraria/Recipe.cs").read_text())
    tile_item = {}
    for item, tile, style in sorted(placing_items(item_cs)):
        if style == 0 and tile not in tile_item and item in names:
            tile_item[tile] = item
    used = set()
    for f in (root / "crates/app/src").glob("*.rs"):
        used |= {int(i) for i in USED_RE.findall(f.read_text())}
    ids = sorted(used | set(tile_item.values()))
    lines = [
        "//! Item ids, generated by tools/gen_item_names.py from the local",
        "//! decompilation of the player's own game. Do not edit by hand.",
        "",
        "/// `(tile, item)`: the first item that places style 0 of each tile, by tile.",
        "pub const TILE_ITEM: &[(u16, u16)] = &[",
    ]
    lines += [f"    ({t}, {i})," for t, i in sorted(tile_item.items())]
    lines += ["];", ""]
    lines += [f"pub const {k.upper()}: u16 = 1 << {n};" for n, k in enumerate(FLAGS)]
    lines += [
        "",
        "/// What the tooltip shows of an item, from its `SetDefaults`.",
        "#[derive(Clone, Copy, Debug, Default)]",
        "pub struct Stats {",
        "    pub rare: i8,",
        "    pub damage: i16,",
        "    pub crit: i8,",
        "    pub knock_back: f32,",
        "    pub use_style: i8,",
        "    pub use_animation: i16,",
        "    pub pick: i16,",
        "    pub axe: i16,",
        "    pub hammer: i16,",
        "    pub tile_boost: i8,",
        "    pub defense: i16,",
        "    pub heal_life: i16,",
        "    pub mana: i16,",
        "    pub shoot_speed: f32,",
        "    pub max_stack: u16,",
        "    /// `headSlot`, `bodySlot`, `legSlot`: the armour picture it is drawn with, 0 for none.",
        "    pub head_slot: i16,",
        "    pub body_slot: i16,",
        "    pub leg_slot: i16,",
        "    pub flags: u16,",
        "}",
        "",
        "/// `(item, ItemID name, stats)` of the items above and of every `Item_N` sheet the engine draws.",
        "pub const ITEMS: &[(u16, &str, Stats)] = &[",
    ]
    lines += [f'    ({i}, "{names[i]}", Stats {{ {stats(defaults(i), i in material)} }}),' for i in ids if i in names]
    lines += [
        "];",
        "",
        "pub fn stats(id: u16) -> Option<Stats> {",
        "    ITEMS.binary_search_by_key(&id, |e| e.0).ok().map(|i| ITEMS[i].2)",
        "}",
        "",
        "/// `(ItemID name, maxStack)` of every item, by id: what a world's chests may hold.",
        "pub const ALL: &[(&str, u16)] = &[",
    ]
    top = max(names)
    lines += [f'    ("{names.get(i, "")}", {max_stack(defaults(i)) if i in names else 0}),'
              for i in range(top + 1)]
    lines += ["];", ""]
    (root / "crates/app/src/item_ids.rs").write_text("\n".join(lines))


main()
