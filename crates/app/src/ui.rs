//! The interface as the game draws it: the hotbar of `GUIHotbarDrawInner` and
//! `ItemSlot.Draw`, the classic life hearts of `ClassicPlayerResourcesDisplaySet`,
//! and text in the game's own fonts.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use formats::xnb::{self, Font, Texture};
use render::atlas::SheetRef;
use render::Sprite;

use crate::inventory::HOTBAR;

/// `Inventory_Back`'s size.
const BACK: f32 = 52.0;
/// Items bigger than this are shrunk to fit a slot (`DrawItemIcon`'s size limit).
pub const ICON_LIMIT: f32 = 32.0;
const MANA_PER_STAR: i32 = 20;
const LIFE_PER_HEART: i32 = 20;

pub struct Fonts {
    pub item_stack: Option<Font>,
    pub mouse_text: Option<Font>,
    pub combat_text: Option<Font>,
    pub combat_crit: Option<Font>,
}

#[derive(Clone, Copy)]
pub enum FontId {
    ItemStack,
    MouseText,
    CombatText,
    CombatCrit,
}

impl FontId {
    fn file(self) -> &'static str {
        match self {
            FontId::ItemStack => "Item_Stack",
            FontId::MouseText => "Mouse_Text",
            FontId::CombatText => "Combat_Text",
            FontId::CombatCrit => "Combat_Crit",
        }
    }
}

fn page_sheet(id: FontId, page: usize) -> String {
    format!("Font_{}_{page}", id.file())
}

impl Fonts {
    /// Reads the Latin glyphs of the fonts under `dir` (`Content/Fonts`); their
    /// pages are returned to be packed into the atlas.
    /// Latin, and every other character `chars` (the loaded texts) holds.
    pub fn load(dir: Option<&Path>, chars: &HashSet<char>) -> (Self, Vec<(String, Texture)>) {
        let mut pages = Vec::new();
        let mut read = |id: FontId| {
            let data = std::fs::read(dir?.join(format!("{}.xnb", id.file()))).ok()?;
            let mut font = xnb::read_font(&data, |c| c < '\u{250}' || chars.contains(&c)).ok()?;
            for (i, p) in font.pages.iter_mut().enumerate().filter(|(_, p)| !p.rgba.is_empty()) {
                pages
                    .push((page_sheet(id, i), std::mem::replace(p, Texture { width: 0, height: 0, rgba: Vec::new() })));
            }
            Some(font)
        };
        let fonts = Fonts {
            item_stack: read(FontId::ItemStack),
            mouse_text: read(FontId::MouseText),
            combat_text: read(FontId::CombatText),
            combat_crit: read(FontId::CombatCrit),
        };
        (fonts, pages)
    }

    fn get(&self, id: FontId) -> Option<&Font> {
        match id {
            FontId::ItemStack => self.item_stack.as_ref(),
            FontId::MouseText => self.mouse_text.as_ref(),
            FontId::CombatText => self.combat_text.as_ref(),
            FontId::CombatCrit => self.combat_crit.as_ref(),
        }
    }
}

/// Interface state the game animates every tick.
pub struct Hud {
    hotbar_scale: [f32; HOTBAR],
    /// `Main.mouseTextColor`: pulses between 190 and 255.
    text_color: u8,
    text_up: bool,
    /// `Main.cursorAlpha`, which sets `cursorScale`.
    cursor_alpha: f32,
    cursor_up: bool,
    /// `Main.invAlpha`: the inventory backs pulse between 180 and 240.
    inv_alpha: f32,
    inv_up: bool,
    tick_acc: f32,
}

impl Default for Hud {
    fn default() -> Self {
        Self {
            hotbar_scale: [0.75; HOTBAR],
            text_color: 255,
            text_up: false,
            cursor_alpha: 1.0,
            cursor_up: false,
            inv_alpha: 180.0,
            inv_up: true,
            tick_acc: 0.0,
        }
    }
}

impl Hud {
    /// Runs the ticks `dt` holds and returns how many. The hotbar only animates while shown.
    pub fn update(&mut self, dt: f32, selected: usize, hotbar: bool) -> u32 {
        self.tick_acc = (self.tick_acc + dt * 60.0).min(4.0);
        let mut n = 0;
        while self.tick_acc >= 1.0 {
            self.tick_acc -= 1.0;
            self.tick(if hotbar { Some(selected) } else { None });
            n += 1;
        }
        n
    }

    fn tick(&mut self, hotbar: Option<usize>) {
        self.inv_alpha += if self.inv_up { 0.2 } else { -0.2 };
        if self.inv_alpha > 240.0 {
            (self.inv_alpha, self.inv_up) = (240.0, false);
        }
        if self.inv_alpha < 180.0 {
            (self.inv_alpha, self.inv_up) = (180.0, true);
        }
        self.text_color = if self.text_up { self.text_color + 1 } else { self.text_color - 1 };
        if self.text_color == 255 {
            self.text_up = false;
        }
        if self.text_color <= 190 {
            self.text_up = true;
        }
        self.cursor_alpha += if self.cursor_up { 0.015 } else { -0.015 };
        if self.cursor_alpha >= 1.0 {
            (self.cursor_alpha, self.cursor_up) = (1.0, false);
        }
        if self.cursor_alpha <= 0.6 {
            (self.cursor_alpha, self.cursor_up) = (0.6, true);
        }
        let Some(selected) = hotbar else { return };
        for (i, s) in self.hotbar_scale.iter_mut().enumerate() {
            if i == selected {
                if *s < 1.0 {
                    *s += 0.05;
                }
            } else if *s > 0.75 {
                *s -= 0.05;
            }
        }
    }

    fn cursor_scale(&self) -> f32 {
        self.cursor_alpha * 0.3 + 0.8
    }

    /// Top-left and scale of each hotbar slot, in screen pixels.
    pub fn hotbar_slots(&self) -> [([f32; 2], f32); HOTBAR] {
        let mut x = 20;
        std::array::from_fn(|i| {
            let s = self.hotbar_scale[i];
            let at = [x as f32, (20.0 + 22.0 * (1.0 - s)) as i32 as f32];
            x += (BACK * s) as i32 + 4;
            (at, s)
        })
    }

    pub fn hotbar_under(&self, p: [f32; 2]) -> Option<usize> {
        self.hotbar_slots()
            .iter()
            .position(|&([x, y], s)| p[0] >= x && p[0] <= x + BACK * s && p[1] >= y && p[1] <= y + BACK * s)
    }
}

/// `Main.stackSplit` and its speed-up: how fast a held right button takes items.
#[derive(Default)]
pub struct StackSplit {
    split: i32,
    delay: i32,
    counter: i32,
    tries: i32,
    super_fast: i32,
}

impl StackSplit {
    /// One tick; `held`: either mouse button is down.
    pub fn tick(&mut self, held: bool) {
        if !held {
            self.split = 0;
        }
        if self.split > 0 {
            self.split -= 1;
        }
        if self.split == 0 {
            (self.tries, self.counter, self.delay, self.super_fast) = (0, 0, 7, 0);
            return;
        }
        self.counter += 1;
        let every = match self.delay {
            7 => 30,
            6 => 25,
            5 => 20,
            4 => 15,
            3 => 10,
            _ => 5,
        };
        if self.counter >= every {
            self.delay -= 1;
            if self.delay < 2 {
                self.delay = 2;
                self.super_fast += 1;
            }
            if self.tries < 10 {
                self.super_fast = 0;
            }
            self.tries += 1;
            self.counter = 0;
        }
    }

    /// How many items a right press may take this tick.
    pub fn takes(&self) -> i32 {
        if self.split > 1 {
            0
        } else {
            self.super_fast + 1
        }
    }

    /// `RefreshStackSplitCooldown` after an item was taken.
    pub fn took(&mut self) {
        self.split = if self.split == 0 { 30 } else { self.delay };
    }
}

/// `Main.inventoryScale` while the inventory is open.
const INV_SCALE: f32 = 0.85;

/// Top-left of inventory slot `i` (ten per row), in screen pixels.
pub fn inventory_slot(i: usize) -> [f32; 2] {
    let at = |n: usize| (20.0 + (n * 56) as f32 * INV_SCALE) as i32 as f32;
    [at(i % HOTBAR), at(i / HOTBAR)]
}

pub fn inventory_under(p: [f32; 2], slots: usize) -> Option<usize> {
    (0..slots).find(|&i| {
        let [x, y] = inventory_slot(i);
        p[0] >= x && p[0] <= x + BACK * INV_SCALE && p[1] >= y && p[1] <= y + BACK * INV_SCALE
    })
}

/// Bottom of the inventory grid of `slots`, in screen pixels.
pub fn inventory_bottom(slots: usize) -> f32 {
    inventory_slot(slots - 1)[1] + BACK * INV_SCALE
}

/// Top-left of armour slot `k` (head, body, legs) on a screen `w` wide.
pub fn equip_slot(w: f32, k: usize) -> [f32; 2] {
    [(w as i32 - 64 - 28) as f32, (174.0 + (k * 56) as f32 * INV_SCALE) as i32 as f32]
}

pub fn equip_under(p: [f32; 2], w: f32) -> Option<usize> {
    (0..3).find(|&k| {
        let [x, y] = equip_slot(w, k);
        p[0] >= x && p[0] <= x + BACK * INV_SCALE && p[1] >= y && p[1] <= y + BACK * INV_SCALE
    })
}

/// `DrawDefenseCounter`'s centre, beside the last armour row.
fn defense_center(w: f32) -> [f32; 2] {
    let [x, y] = equip_slot(w, 2);
    [x - 10.0 - 47.0 - 47.0 - 14.0, y + BACK * 0.5]
}

/// Whether `p` is over the defense shield.
pub fn defense_under(sheets: &HashMap<String, SheetRef>, p: [f32; 2], w: f32) -> bool {
    let Some(s) = sheets.get("Extra_58") else { return false };
    let ([cx, cy], hw, hh) = (defense_center(w), (s.w / 3) as f32 / 2.0, (s.h / 2) as f32 / 2.0);
    (p[0] - cx).abs() < hw && (p[1] - cy).abs() < hh
}

/// Draws in screen pixels onto world sprites under the camera.
pub struct Painter<'a> {
    pub sheets: &'a HashMap<String, SheetRef>,
    pub fonts: &'a Fonts,
    /// World position of the screen's top-left, and world pixels per screen pixel.
    pub origin: [f32; 2],
    pub unit: f32,
    pub out: Vec<Sprite>,
}

impl Painter<'_> {
    pub fn world(&self, p: [f32; 2]) -> [f32; 2] {
        [self.origin[0] + p[0] * self.unit, self.origin[1] + p[1] * self.unit]
    }

    pub fn rect(&mut self, p: [f32; 2], w: f32, h: f32, color: [f32; 4]) {
        self.out.push(Sprite::flat(self.world(p), [w * self.unit, h * self.unit], color));
    }

    /// A whole sheet scaled about `origin` (in its own pixels), drawn with its origin at `p`.
    pub fn image(&mut self, name: &str, p: [f32; 2], origin: [f32; 2], scale: f32, color: [f32; 4]) {
        let Some(sheet) = self.sheets.get(name) else { return };
        let mut s = Sprite::sheet([0.0; 2], sheet, [0, 0, sheet.w, sheet.h], 0);
        s.pos = self.world([p[0] - origin[0] * scale, p[1] - origin[1] * scale]);
        s.size = [sheet.w as f32 * scale * self.unit, sheet.h as f32 * scale * self.unit];
        s.color = color;
        self.out.push(s);
    }

    /// `src` of a sheet stretched over `size` at `p`.
    pub fn region(&mut self, name: &str, src: [u32; 4], p: [f32; 2], size: [f32; 2], color: [f32; 4]) {
        let Some(sheet) = self.sheets.get(name) else { return };
        let mut s = Sprite::sheet(self.world(p), sheet, src, 0);
        s.size = [size[0] * self.unit, size[1] * self.unit];
        s.color = color;
        self.out.push(s);
    }

    /// `Utils.DrawInvBG`: `Inventory_Back13` cut in nine, its 10 px corners kept.
    pub fn inv_bg(&mut self, x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        let Some(&SheetRef { w: tw, h: th, .. }) = self.sheets.get("Inventory_Back13") else { return };
        let (w, h) = (w.max(20.0), h.max(20.0));
        let cols = [(0, x, 10.0), (10, x + 10.0, w - 20.0), (tw - 10, x + w - 10.0, 10.0)];
        let rows = [(0, y, 10.0), (10, y + 10.0, h - 20.0), (th - 10, y + h - 10.0, 10.0)];
        for (sy, py, ph) in rows {
            for (sx, px, pw) in cols {
                self.region("Inventory_Back13", [sx, sy, 10, 10], [px, py], [pw, ph], color);
            }
        }
    }

    fn glyphs(&mut self, id: FontId, text: &str, p: [f32; 2], scale: f32, color: [f32; 4]) {
        self.string(id, text, p, [0.0; 2], scale, 0.0, color);
    }

    /// `DrawString` with an origin (in text pixels) placed at `p`, scaled and turned about it.
    #[allow(clippy::too_many_arguments)]
    pub fn string(
        &mut self,
        id: FontId,
        text: &str,
        p: [f32; 2],
        origin: [f32; 2],
        scale: f32,
        rot: f32,
        color: [f32; 4],
    ) {
        let Some(font) = self.fonts.get(id) else { return };
        for ([x, y], g) in font.layout(text) {
            let Some(sheet) = self.sheets.get(&page_sheet(id, g.page)) else { continue };
            let [gx, gy, gw, gh] = g.rect.map(|v| v.max(0) as u32);
            let mut s = Sprite::sheet([0.0; 2], sheet, [gx, gy, gw, gh], 0);
            let at = [p[0] + (x - origin[0]) * scale, p[1] + (y - origin[1]) * scale];
            s.pos = self.world(at);
            s.size = [gw as f32 * scale * self.unit, gh as f32 * scale * self.unit];
            s.color = color;
            s.rot = rot;
            s.pivot = [(p[0] - at[0]) * self.unit, (p[1] - at[1]) * self.unit];
            self.out.push(s);
        }
    }

    /// `DrawColorCodedStringWithShadow`: the text over four black copies `spread` away.
    pub fn text(&mut self, id: FontId, text: &str, p: [f32; 2], scale: f32, color: [f32; 4], spread: f32) {
        let shadow = [0.0, 0.0, 0.0, color[3]];
        for [dx, dy] in [[-1.0, 0.0], [1.0, 0.0], [0.0, -1.0], [0.0, 1.0]] {
            self.glyphs(id, text, [p[0] + dx * spread, p[1] + dy * spread], scale, shadow);
        }
        self.glyphs(id, text, p, scale, color);
    }

    /// Plain `DrawString`.
    pub fn plain_text(&mut self, id: FontId, text: &str, p: [f32; 2], color: [f32; 4]) {
        self.glyphs(id, text, p, 1.0, color);
    }

    pub fn measure(&self, id: FontId, text: &str) -> [f32; 2] {
        self.fonts.get(id).map_or([0.0; 2], |f| f.measure(text).into())
    }
}

fn gray(v: f32, a: f32) -> [f32; 4] {
    [v, v, v, a]
}

impl Hud {
    /// `GUIHotbarDrawInner` with `ItemSlot.Draw` for context 13.
    /// `counts`: the stack in each slot (0 when empty); `icon` draws slot `i`'s item
    /// centred on a screen point at a scale.
    pub fn draw_hotbar(
        &self,
        p: &mut Painter,
        selected: usize,
        counts: &[u16],
        title: &str,
        icon: impl Fn(&mut Painter, usize, [f32; 2], f32),
    ) {
        let c = self.text_color as f32 / 255.0;
        let w = p.measure(FontId::MouseText, title)[0];
        p.plain_text(FontId::MouseText, title, [236.0 - w / 2.0, 0.0], gray(c, c));
        for (i, &(at, s)) in self.hotbar_slots().iter().enumerate() {
            let light = gray(1.0, (75.0 + 150.0 * s) as i32 as f32 / 255.0);
            let (back, b) = if i == selected { ("Inventory_Back14", 1.0) } else { ("Inventory_Back", 200.0 / 255.0) };
            p.image(back, at, [0.0; 2], s, gray(b, b));
            let count = counts.get(i).copied().unwrap_or(0);
            if count > 0 {
                icon(p, i, [at[0] + BACK * s / 2.0, at[1] + BACK * s / 2.0], s);
            }
            if count > 1 {
                let at = [at[0] + 10.0 * s, at[1] + 26.0 * s];
                p.text(FontId::ItemStack, &count.to_string(), at, s, light, s);
            }
            let n = ((i + 1) % 10).to_string();
            p.text(FontId::ItemStack, &n, [at[0] + 8.0 * s, at[1] + 4.0 * s], s, light, s);
        }
    }

    /// `DrawInventory`'s grid with `ItemSlot.Draw` for context 0; `label` is the game's "Inventory".
    pub fn draw_inventory(
        &self,
        p: &mut Painter,
        label: &str,
        counts: &[u16],
        icon: impl Fn(&mut Painter, usize, [f32; 2], f32),
    ) {
        let c = self.text_color as f32 / 255.0;
        p.plain_text(FontId::MouseText, label, [40.0, 0.0], gray(c, c));
        let back = (self.inv_alpha as u8) as f32 / 255.0;
        for (i, &count) in counts.iter().enumerate() {
            let at = inventory_slot(i);
            let name = if i < HOTBAR { "Inventory_Back9" } else { "Inventory_Back" };
            p.image(name, at, [0.0; 2], INV_SCALE, gray(back, back));
            self.slot_item(p, at, INV_SCALE, count, |p, c, s| icon(p, i, c, s));
        }
    }

    /// The armour slots of `DrawInventory` (`ItemSlot.Draw` for context 8): the
    /// loadout's green back, the piece or its faded outline, and the defense.
    pub fn draw_equipment(
        &self,
        p: &mut Painter,
        w: f32,
        worn: [bool; 3],
        defense: i32,
        icon: impl Fn(&mut Painter, usize, [f32; 2], f32),
    ) {
        let a = (self.inv_alpha as u8) as f32 / 255.0;
        let back = [50.0 / 255.0 * a, 106.0 / 255.0 * a, 64.0 / 255.0 * a, a];
        for (k, &on) in worn.iter().enumerate() {
            let at = equip_slot(w, k);
            p.image("Inventory_Back13", at, [0.0; 2], INV_SCALE, back);
            if on {
                self.slot_item(p, at, INV_SCALE, 1, |p, c, s| icon(p, k, c, s));
            } else if let Some(&SheetRef { w: tw, h: th, .. }) = p.sheets.get("Extra_54") {
                let (fw, fh) = (tw / 3, th / 7);
                let src = [0, (k as u32 * 6 / 3) * fh, fw - 2, fh - 2];
                let size = [(fw - 2) as f32 * INV_SCALE, (fh - 2) as f32 * INV_SCALE];
                let c = [at[0] + BACK * INV_SCALE / 2.0, at[1] + BACK * INV_SCALE / 2.0];
                p.region("Extra_54", src, [c[0] - size[0] / 2.0, c[1] - size[1] / 2.0], size, gray(0.35, 0.35));
            }
        }
        let Some(&SheetRef { w: tw, h: th, .. }) = p.sheets.get("Extra_58") else { return };
        let (fw, fh) = (tw / 3, th / 2);
        let size = [fw as f32 * INV_SCALE, fh as f32 * INV_SCALE];
        let c = defense_center(w);
        p.region("Extra_58", [0, 0, fw, fh], [c[0] - size[0] / 2.0, c[1] - size[1] / 2.0], size, [1.0; 4]);
        let text = defense.to_string();
        let [tw, th] = p.measure(FontId::MouseText, &text);
        let at = [c[0] - tw * 0.5 * INV_SCALE, c[1] - th * 0.5 * INV_SCALE + 2.0 * INV_SCALE];
        p.text(FontId::MouseText, &text, at, INV_SCALE, [1.0; 4], 2.0);
    }

    /// The item and its stack number in a slot whose top-left is `at`.
    fn slot_item(&self, p: &mut Painter, at: [f32; 2], s: f32, count: u16, icon: impl Fn(&mut Painter, [f32; 2], f32)) {
        if count > 0 {
            icon(p, [at[0] + BACK * s / 2.0, at[1] + BACK * s / 2.0], s);
        }
        if count > 1 {
            p.text(FontId::ItemStack, &count.to_string(), [at[0] + 10.0 * s, at[1] + 26.0 * s], s, [1.0; 4], s);
        }
    }

    /// `DrawInterface_38_MouseCarriedObject`: the carried item at the cursor, without a back.
    pub fn draw_mouse_item(
        &self,
        p: &mut Painter,
        cursor: [f32; 2],
        count: u16,
        icon: impl Fn(&mut Painter, [f32; 2], f32),
    ) {
        self.slot_item(p, cursor, self.cursor_scale(), count, icon);
    }

    /// `MouseText_DrawItemTooltip` with the opaque box the game draws by default.
    pub fn draw_item_tooltip(
        &self,
        p: &mut Painter,
        cursor: [f32; 2],
        screen: [f32; 2],
        lines: &[(String, [u8; 3])],
        carrying: bool,
    ) {
        let sizes: Vec<[f32; 2]> = lines.iter().map(|l| p.measure(FontId::MouseText, &l.0)).collect();
        let w = sizes.iter().map(|s| s[0]).fold(0.0, f32::max);
        let h: f32 = sizes.iter().map(|s| s[1]).sum();
        let mut x = cursor[0] as i32 + 14 + if carrying { 34 } else { 0 } + 6 + 8;
        let mut y = cursor[1] as i32 + 14 + 6 + 2;
        if x as f32 + w + 18.0 > screen[0] {
            x = (screen[0] - w - 18.0) as i32;
        }
        if y as f32 + h + 18.0 > screen[1] {
            y = (screen[1] - h - 18.0) as i32;
        }
        let (x, y) = (x as f32, y as f32);
        let k = 0.925 / 255.0;
        p.inv_bg(
            x - 14.0,
            y - 9.0,
            (w as i32 + 28) as f32,
            (h as i32 + 13) as f32,
            [23.0 * k, 25.0 * k, 81.0 * k, 0.925],
        );
        let c = self.text_color as f32 / 255.0;
        let mut dy = 0.0;
        for ((text, rgb), size) in lines.iter().zip(&sizes) {
            let color = [rgb[0] as f32 * c / 255.0, rgb[1] as f32 * c / 255.0, rgb[2] as f32 * c / 255.0, c];
            p.text(FontId::MouseText, text, [x, y + dy], 1.0, color, 2.0);
            dy += size[1].floor();
        }
    }

    /// `ClassicPlayerResourcesDisplaySet.DrawLife` for up to 400 life (no golden hearts).
    /// `label`: the game's "Life:".
    pub fn draw_life(&self, p: &mut Painter, screen_w: f32, life: i32, life_max: i32, label: &str) {
        let anchor = screen_w - 800.0;
        let hearts = (life_max / LIFE_PER_HEART).min(10);
        let c = self.text_color as f32 / 255.0;
        let w = p.measure(FontId::MouseText, &format!("{label} {life_max}/{life_max}"))[0];
        let mid = 500.0 + 13.0 * hearts as f32 + anchor;
        p.plain_text(FontId::MouseText, label, [mid - w / 2.0, 6.0], gray(c, c));
        let numbers = format!("{life}/{life_max}");
        let nw = p.measure(FontId::MouseText, &numbers)[0];
        p.plain_text(FontId::MouseText, &numbers, [mid + w / 2.0 - nw, 6.0], gray(c, c));
        let size = p.sheets.get("Heart").map_or(22.0, |s| s.h as f32);
        let per = LIFE_PER_HEART as f32;
        for i in 1..=life_max / LIFE_PER_HEART {
            let (alpha, mut scale, pulse) = if life as f32 >= i as f32 * per {
                (255, 1.0, life as f32 == i as f32 * per)
            } else {
                let part = (life as f32 - (i - 1) as f32 * per) / per;
                (((30.0 + 225.0 * part) as i32).max(30), (part / 4.0 + 0.75).max(0.75), part > 0.0)
            };
            if pulse {
                scale += self.cursor_scale() - 1.0;
            }
            let (dx, dy) = if i > 10 { (-260.0, 26.0) } else { (0.0, 0.0) };
            let x = 500.0 + 26.0 * (i - 1) as f32 + dx + anchor + (size / 2.0).floor();
            let y = 32.0 + (size - size * scale) / 2.0 + dy + (size / 2.0).floor();
            let v = alpha as f32 / 255.0;
            p.image(
                "Heart",
                [x, y],
                [(size / 2.0).floor(); 2],
                scale,
                gray(v, (alpha as f32 * 0.9) as i32 as f32 / 255.0),
            );
        }
    }

    /// `ClassicPlayerResourcesDisplaySet.DrawMana`: a star per 20 mana under `label` ("Mana").
    pub fn draw_mana(&self, p: &mut Painter, screen_w: f32, mana: i32, mana_max: i32, label: &str) {
        let anchor = screen_w - 800.0;
        let c = self.text_color as f32 / 255.0;
        let w = p.measure(FontId::MouseText, label)[0];
        let left = if w >= 45.0 { w as i32 + 5 } else { 50 };
        p.plain_text(FontId::MouseText, label, [800.0 - left as f32 + anchor, 6.0], gray(c, c));
        let Some(&SheetRef { w: sw, h: sh, .. }) = p.sheets.get("Mana") else { return };
        let (sw, sh) = (sw as f32, sh as f32);
        for i in 1..=mana_max / MANA_PER_STAR {
            let (alpha, mut scale, pulse) = if mana >= i * MANA_PER_STAR {
                (255, 1.0, mana == i * MANA_PER_STAR)
            } else {
                let part = (mana - (i - 1) * MANA_PER_STAR) as f32 / MANA_PER_STAR as f32;
                (((30.0 + 225.0 * part) as i32).max(30), (part / 4.0 + 0.75).max(0.75), part > 0.0)
            };
            if pulse {
                scale += self.cursor_scale() - 1.0;
            }
            let y = 30.0 + (sh / 2.0).floor() + (sh - sh * scale) / 2.0 + 28.0 * (i - 1) as f32;
            let v = alpha as f32 / 255.0;
            let origin = [(sw / 2.0).floor(), (sh / 2.0).floor()];
            p.image("Mana", [775.0 + anchor, y], origin, scale, gray(v, (alpha as f32 * 0.9) as i32 as f32 / 255.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_slot_grows_and_pushes_the_others() {
        let mut h = Hud::default();
        let before = h.hotbar_slots();
        for _ in 0..10 {
            h.tick(Some(2));
        }
        let after = h.hotbar_slots();
        assert!((after[2].1 - 1.0).abs() < 1e-3, "{}", after[2].1);
        assert_eq!(after[2].0[1], 20.0, "a full slot sits at the top");
        assert_eq!(before[3].0[1], 25.0);
        assert_eq!(after[3].0[0] - after[2].0[0], 56.0, "52 wide and 4 apart");
        assert_eq!(h.hotbar_under([after[2].0[0] + 1.0, 30.0]), Some(2));
        assert_eq!(h.hotbar_under([5.0, 5.0]), None);
    }

    #[test]
    fn held_right_button_speeds_up() {
        let mut s = StackSplit::default();
        let mut taken = Vec::new();
        for t in 0..400 {
            s.tick(true);
            for _ in 0..s.takes() {
                s.took();
                taken.push(t);
            }
        }
        assert_eq!(&taken[..2], &[0, 29], "one, then a 30 tick wait");
        let gaps: Vec<i32> = taken.windows(2).map(|w| w[1] - w[0]).collect();
        assert!(gaps.last() < gaps.get(1), "faster as it is held: {gaps:?}");
        s.tick(false);
        assert_eq!(s.takes(), 1);
    }

    #[test]
    fn inventory_grid_is_the_games() {
        assert_eq!(inventory_slot(0), [20.0, 20.0]);
        assert_eq!(inventory_slot(11), [67.0, 67.0], "56 * 0.85 apart");
        assert_eq!(inventory_under([70.0, 70.0], 50), Some(11));
        assert_eq!(inventory_under([500.0, 500.0], 50), None);
    }

    #[test]
    fn text_colour_pulses() {
        let mut h = Hud::default();
        for _ in 0..65 {
            h.tick(Some(0));
        }
        assert_eq!(h.text_color, 190);
        h.tick(Some(0));
        assert_eq!(h.text_color, 191);
    }
}
