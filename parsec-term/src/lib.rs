#![feature(result_option_inspect, iter_collect_into)]

use std::{
    fmt::Debug,
    io,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc
    }
};

use area::Coord;
use crossterm::{
    cursor, execute,
    terminal::{self, ClearType}
};

use label::PrintInfo;
use parsec_core::{
    data::RwData,
    log_info,
    ui::{self, Area as UiArea, Axis, Side, Split}
};

mod area;
mod label;
mod rules;

fn scale_children(children: &[(Node, Split)], len_diff: i16, axis: Axis) -> Vec<u16> {
    let mut lens = children
        .iter()
        .filter_map(|(node, split)| {
            if let Split::Min(_) = *split {
                Some(node.area.len(axis) as u16)
            } else {
                None
            }
        })
        .collect::<Vec<u16>>();

    let old_len = lens.iter().sum::<u16>();

    let diff_array = {
        let abs_diff = len_diff.abs() as usize;
        let mut vec = Vec::with_capacity(abs_diff);
        for element in 0..abs_diff {
            let ratio = element as f32 / abs_diff as f32;
            vec.push((ratio * old_len as f32).round() as u16);
        }

        vec
    };

    let mut accum = 0;
    let mut last_diff = 0;
    for len in lens.iter_mut() {
        let next_accum = accum + *len;

        for diff in &diff_array[last_diff..] {
            if (accum..next_accum).contains(diff) {
                *len = len.saturating_add_signed(len_diff.signum());
                last_diff += 1;
            }
        }

        accum = next_accum
    }

    lens
}

#[derive(Debug)]
pub enum Anchor {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight
}

pub struct Node {
    area: Area,
    lineage: Option<(Vec<(Node, Split)>, Axis)>
}

impl Node {
    pub fn new(area: Area, lineage: Option<(Vec<(Node, Split)>, Axis)>) -> Self {
        Self { area, lineage }
    }

    /// The length of [`self`] that can be resized, in a given
    /// [`Axis`].
    ///
    /// This length consists of the total length that is not contained
    /// within children whose split is [`Split::Locked(_)`].
    fn resizable_len(&self, axis: Axis, window: &InnerWindow) -> usize {
        if let Axis::Horizontal = axis {
            self.resizable_width(window, true)
        } else {
            self.resizable_height(window, true)
        }
    }

    /// The width of [`self`] that can be resized.
    ///
    /// This width consists of the total width that is not contained
    /// within children whose split is [`Split::Locked(_)`].
    fn resizable_width(&self, window: &InnerWindow, is_root: bool) -> usize {
        if !is_root {
            if let Some((child_index, parent)) = window.find_parent(self.area.index) {
                let (children, axis) = parent.lineage.as_ref().unwrap();
                if let (Axis::Horizontal, Split::Locked(_)) = (axis, children[child_index].1) {
                    return 0;
                }
            };
        }
        drop(window);

        if let Some((children, ..)) = &self.lineage {
            if children.is_empty() {
                self.area.width()
            } else {
                children.iter().map(|(node, _)| node.resizable_width(window, false)).sum()
            }
        } else {
            self.area.width()
        }
    }

    /// The height of [`self`] that can be resized.
    ///
    /// This height consists of the total height that is not contained
    /// within children whose split is [`Split::Locked(_)`].
    fn resizable_height(&self, window: &InnerWindow, is_root: bool) -> usize {
        if !is_root {
            if let Some((child_index, parent)) = window.find_parent(self.area.index) {
                let (children, axis) = parent.lineage.as_ref().unwrap();
                if let (Axis::Horizontal, Split::Locked(_)) = (axis, children[child_index].1) {
                    return 0;
                }
            };
        }
        drop(window);

        if let Some((children, ..)) = &self.lineage {
            if children.is_empty() {
                self.area.height()
            } else {
                children.iter().map(|(node, _)| node.resizable_height(window, false)).sum()
            }
        } else {
            self.area.height()
        }
    }

    fn find_node(&self, area_index: usize) -> Option<&Self> {
        if self.area.index == area_index {
            Some(self)
        } else {
            if let Some((children, _)) = &self.lineage {
                for (child, _) in children {
                    let area = child.find_node(area_index);
                    if area.is_some() {
                        return area;
                    }
                }
            }

            None
        }
    }

    fn find_mut_node(&mut self, area_index: usize) -> Option<&mut Self> {
        if self.area.index == area_index {
            Some(self)
        } else {
            if let Some((children, _)) = &mut self.lineage {
                for (child, _) in children {
                    let node = child.find_mut_node(area_index);
                    if node.is_some() {
                        return node;
                    }
                }
            }

            None
        }
    }

    fn find_parent_index(&self, area_index: usize) -> Option<(usize, usize)> {
        if let Some((children, _)) = &self.lineage {
            match children.iter().position(|(node, _)| node.area.index == area_index) {
                Some(index) => {
                    drop(children);
                    return Some((index, self.area.index));
                }
                None => {
                    for (node, _) in children {
                        if let Some(ret) = node.find_parent_index(area_index) {
                            return Some(ret);
                        }
                    }

                    None
                }
            }
        } else {
            None
        }
    }

    fn find_parent(&self, area_index: usize) -> Option<(usize, &Node)> {
        let parent = if let Some((children, _)) = &self.lineage {
            match children.iter().position(|(node, _)| node.area.index == area_index) {
                Some(index) => Some((index, self.area.index)),
                None => {
                    let mut parent = None;
                    for (node, _) in children {
                        if let Some(ret) = node.find_parent_index(area_index) {
                            parent = Some(ret);
                        }
                    }

                    parent
                }
            }
        } else {
            None
        };

        parent
            .map(|(child_index, parent_index)| (child_index, self.find_node(parent_index).unwrap()))
    }

    fn find_mut_parent(&mut self, area_index: usize) -> Option<(usize, &mut Self)> {
        let parent = if let Some((children, _)) = &mut self.lineage {
            match children.iter().position(|(node, _)| node.area.index == area_index) {
                Some(index) => Some((index, self.area.index)),
                None => {
                    let mut parent = None;
                    for (node, _) in children {
                        if let Some(ret) = node.find_parent_index(area_index) {
                            parent = Some(ret);
                        }
                    }

                    parent
                }
            }
        } else {
            None
        };

        parent.map(|(child_index, parent_index)| {
            (child_index, self.find_mut_node(parent_index).unwrap())
        })
    }

    // NOTE: This function will only be called once it is known that the
    // parent has enough length to accomodate the specific resize.
    /// Tries to set the length of a given `index` to `new_len`,
    /// expanding from [`side`][Side].
    ///
    /// Returns [`Ok(())`] if the child's length was changed
    /// succesfully, and [`Err(())`] if it wasn't.
    fn set_child_len(&self, child_index: usize, new_len: u16, side: Side) -> Result<(), ()> {
        let (children, axis) = self.lineage.as_ref().unwrap();

        let bef_len = resizable_len_of(&children[..child_index], *axis) as i16;
        let aft_len = if children.len() > child_index {
            resizable_len_of(&children[(child_index + 1)..], *axis) as i16
        } else {
            0
        };

        let old_len = children.get(child_index).unwrap().0.area.len(*axis);
        let len_diff = new_len as i16 - old_len as i16;

        let (bef_diff, aft_diff) = if let Side::Right | Side::Bottom = side {
            let aft_diff = aft_len.min(len_diff);
            let bef_diff = len_diff - aft_diff;

            let mut coords = children.get(child_index).unwrap().0.area.coords.write();
            coords.add_to_side(side, aft_diff);
            coords.add_to_side(side.opposite(), bef_diff);
            (-bef_diff, -aft_diff)
        } else {
            let bef_diff = bef_len.min(len_diff);
            let aft_diff = len_diff - bef_diff;

            let mut coords = children.get(child_index).unwrap().0.area.coords.write();
            coords.add_to_side(side, bef_diff);
            coords.add_to_side(side.opposite(), aft_diff);
            (-bef_diff, -aft_diff)
        };

        let parent_coords = self.area.coords();

        let bef_lens = scale_children(&children[..child_index], bef_diff, *axis);
        let br = children.get(child_index).unwrap().0.area.coords().ortho_corner(axis.perp());
        let coords = Coords::new(parent_coords.tl, br);
        normalize_to_coords(&children[..child_index], bef_lens, coords, Axis::from(side));

        let aft_lens = scale_children(&children[(child_index + 1)..], aft_diff, *axis);
        let tl = children.get(child_index).unwrap().0.area.coords().ortho_corner(*axis);
        let coords = Coords::new(tl, parent_coords.br);
        normalize_to_coords(&children[(child_index + 1)..], aft_lens, coords, Axis::from(side));

        Ok(())
    }

    fn insert_new_area(&mut self, child_index: usize, split: Split, side: Side, node_index: usize) {
        let (children, _) = self.lineage.as_mut().unwrap();

        let coords = children
            .get(child_index)
            .map(|(node, _)| node.area.coords())
            .unwrap_or(self.area.coords())
            .empty_on(side);

        let self_index = match side {
            Side::Bottom | Side::Right => child_index + 1,
            Side::Top | Side::Left => child_index
        };

        let area = Area::new(coords, node_index, self.area.window.clone());
        children.insert(self_index, (Node::new(area, None), split));

        self.set_child_len(self_index, split.len() as u16, side).unwrap();
    }

    /// Normalizes the children of this [`Area`] to fit the resized
    /// length of their parent.
    ///
    /// When an [`Area`] is resized, its [`Coords`] are changed, but
    /// the [`Coords`] of its children still reflect the old
    /// [`Coords`] from their parent. This function's purpose is
    /// to adjust them to the new [`Coords`] of their parent.
    fn normalize_children(&self, len_diff: i16, axis: Axis, perp_len_diff: i16) {
        let Some((children, self_axis)) = &self.lineage else {
            return;
        };

        if *self_axis == axis {
            let new_lens = scale_children(children, len_diff, axis);
            normalize_to_coords(children, new_lens, self.area.coords(), axis);
        } else {
            let new_lens = scale_children(children, perp_len_diff, axis.perp());
            normalize_to_coords(children, new_lens, self.area.coords(), axis.perp());
        }
    }
}

impl Debug for Node {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Area")
            .field("coords", &self.area.coords())
            .field("index", &self.area.index)
            .field("lineage", &self.lineage)
            .finish()
    }
}

fn normalize_to_coords(children: &[(Node, Split)], lens: Vec<u16>, parent: Coords, axis: Axis) {
    let mut last_tl = parent.tl;
    let mut new_lens = lens.iter();
    // let locked_len = children.iter().filter_map(|(_, split)|
    // split.as_locked()).sum::<usize>(); let resizable_len =
    // parent.len(axis) - locked_len;

    log_info!("\n\nnormalizing to {} on {parent}\n", parent.len(axis));

    for (node, split) in children.iter() {
        log_info!("\nsplit: {:?}", split);

        let len = if let Split::Locked(len) = split {
            *len as u16
        } else {
            *new_lens.next().unwrap()
        };

        let (old_len, perp_len_diff) = node.area.coords.mutate(|coords| {
            let old_len = coords.len(axis);
            let perp_old_len = coords.len(axis.perp()) as i16;
            coords.tl = last_tl;
            coords.br = match axis {
                Axis::Horizontal => Coord::new(last_tl.x + len, parent.br.y),
                Axis::Vertical => Coord::new(parent.br.x, last_tl.y + len)
            };

            log_info!(", coords: {coords}");

            last_tl = coords.ortho_corner(axis);

            let perp_len_diff = coords.len(axis.perp()) as i16 - perp_old_len;
            (old_len, perp_len_diff)
        });
        node.normalize_children(len as i16 - old_len as i16, axis, perp_len_diff);
    }
}

// NOTE: This function is meant to be used when the `axis` is aligned
// with the parent's axis.
fn resizable_len_of(children: &[(Node, Split)], axis: Axis) -> u16 {
    children
        .iter()
        .map(|(node, split)| {
            let coords = node.area.coords();
            match *split {
                Split::Min(len) => (coords.len(axis).saturating_sub(len)) as u16,
                Split::Locked(_) => 0
            }
        })
        .sum::<u16>()
}

#[derive(Debug)]
struct InnerWindow {
    main_node: Option<Node>,
    floating_nodes: Vec<(Node, Anchor)>,
    next_index: Arc<AtomicUsize>,
    cur_state: AtomicUsize
}

impl InnerWindow {
    fn find_node(&self, node_index: usize) -> Option<&Node> {
        let floating = self.floating_nodes.iter().map(|(node, _)| node);
        for node in floating.chain(std::iter::once(self.main_node.as_ref().unwrap())) {
            let ret = node.find_node(node_index);
            if ret.is_some() {
                return ret;
            }
        }

        None
    }

    fn find_mut_node(&mut self, node_index: usize) -> Option<&mut Node> {
        let floating = self.floating_nodes.iter_mut().map(|(node, _)| node);
        for node in floating.chain(std::iter::once(self.main_node.as_mut().unwrap())) {
            let ret = node.find_mut_node(node_index);
            if ret.is_some() {
                return ret;
            }
        }

        None
    }

    fn find_parent(&self, area_index: usize) -> Option<(usize, &Node)> {
        let floating = self.floating_nodes.iter().map(|(node, _)| node);
        for node in floating.chain(std::iter::once(self.main_node.as_ref().unwrap())) {
            let ret = node.find_parent(area_index);
            if ret.is_some() {
                return ret;
            }
        }

        None
    }

    fn find_mut_parent(&mut self, node_index: usize) -> Option<(usize, &mut Node)> {
        let floating = self.floating_nodes.iter_mut().map(|(node, _)| node);
        for node in floating.chain(std::iter::once(self.main_node.as_mut().unwrap())) {
            let ret = node.find_mut_parent(node_index);
            if ret.is_some() {
                return ret;
            }
        }

        None
    }
}

pub struct Window {
    inner: RwData<InnerWindow>,
    read_state: AtomicUsize
}

impl Clone for Window {
    fn clone(&self) -> Self {
        Window {
            inner: self.inner.clone(),
            read_state: AtomicUsize::new(self.inner.read().cur_state.load(Ordering::Relaxed))
        }
    }
}

impl ui::Window for Window {
    type Area = Area;
    type Label = Label;

    fn layout_has_changed(&self) -> bool {
        let cur_state = self.inner.read().cur_state.load(Ordering::Relaxed);
        let has_changed = cur_state > self.read_state.load(Ordering::Relaxed);
        self.read_state.store(cur_state, Ordering::Relaxed);
        has_changed
    }

    fn get_area(&self, area_index: usize) -> Option<Self::Area> {
        self.inner.read().find_node(area_index).map(|node| node.area.clone())
    }

    fn get_label(&self, area_index: usize) -> Option<Self::Label> {
        self.inner
            .read()
            .find_node(area_index)
            .filter(|node| node.lineage.is_none())
            .map(|node| Label::new(node.area.clone()))
    }
}

pub struct Ui {
    windows: Vec<Window>,
    next_index: Arc<AtomicUsize>
}

impl Default for Ui {
    fn default() -> Self {
        Ui {
            windows: Vec::new(),
            next_index: Arc::new(AtomicUsize::new(0))
        }
    }
}

impl ui::Ui for Ui {
    type Area = Area;
    type Label = Label;
    type PrintInfo = PrintInfo;
    type Window = Window;

    fn new_window(&mut self) -> (Self::Window, Self::Label) {
        let new_area_index = self.next_index.fetch_add(1, Ordering::Acquire);
        let inner_window = RwData::new(InnerWindow {
            main_node: None,
            floating_nodes: Vec::new(),
            next_index: self.next_index.clone(),
            cur_state: AtomicUsize::new(0)
        });

        let area = Area::total(new_area_index, inner_window.clone());
        let node = Node::new(area.clone(), None);
        inner_window.write().main_node = Some(node);

        let window = Window {
            inner: inner_window,
            read_state: AtomicUsize::new(1)
        };
        self.windows.push(window.clone());

        (window, Label::new(area))
    }

    fn startup(&mut self) {
        // This makes it so that if the application panics, the panic message
        // is printed nicely and the terminal is left in a usable
        // state.
        use std::panic;
        let orig_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            let _ = execute!(
                io::stdout(),
                terminal::Clear(ClearType::All),
                terminal::LeaveAlternateScreen,
                cursor::Show
            );

            terminal::disable_raw_mode().unwrap();

            orig_hook(panic_info);
            std::process::exit(1)
        }));

        let _ = execute!(io::stdout(), terminal::EnterAlternateScreen);
        terminal::enable_raw_mode().unwrap();
    }

    fn shutdown(&mut self) {
        let _ = execute!(
            io::stdout(),
            terminal::Clear(ClearType::All),
            terminal::LeaveAlternateScreen,
            cursor::Show
        );
        terminal::disable_raw_mode().unwrap();
    }
}

pub use area::{Area, Coords};
pub use label::Label;
pub use rules::{SepChar, SepForm, VertRule, VertRuleCfg};
