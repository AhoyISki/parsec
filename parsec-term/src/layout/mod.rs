mod frame;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
};

use cassowary::{
    strength::{MEDIUM, REQUIRED, STRONG, WEAK},
    Constraint as CassowaryConstraint, Expression, Solver, Variable,
    WeightedRelation::*,
};
pub use frame::{Edge, Brush, EdgeCoords, Frame};
use parsec_core::{
    data::RwData,
    ui::{Axis, Constraint, PushSpecs},
};

use crate::{area::Coord, AreaIndex};

/// Generates a unique index for [`Rect`]s.
pub(crate) fn unique_area_index() -> AreaIndex {
    static INDEX_COUNTER: AtomicUsize = AtomicUsize::new(0);

    AreaIndex(INDEX_COUNTER.fetch_add(1, Ordering::SeqCst))
}

/// A [`Variable`], attached to its value, which is automatically kept
/// up to date.
#[derive(Clone)]
struct VarValue {
    var: Variable,
    value: Arc<AtomicUsize>,
    has_changed: Arc<AtomicBool>,
}

impl VarValue {
    /// Returns a new instance of [`VarValue`]
    fn new() -> Self {
        Self {
            var: Variable::new(),
            value: Arc::new(AtomicUsize::new(0)),
            has_changed: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl PartialEq<VarValue> for VarValue {
    fn eq(&self, other: &VarValue) -> bool {
        self.var == other.var
    }
}

impl PartialOrd<VarValue> for VarValue {
    fn partial_cmp(&self, other: &VarValue) -> Option<std::cmp::Ordering> {
        Some(
            self.value
                .load(Ordering::Acquire)
                .cmp(&other.value.load(Ordering::Acquire)),
        )
    }
}

/// A point on the screen, which can be calculated by [`cassowary`]
/// and interpreted by `parsec_term`.
#[derive(Clone, PartialEq, PartialOrd)]
struct VarPoint {
    x: VarValue,
    y: VarValue,
}

impl VarPoint {
    /// Returns a new instance of [`VarPoint`]
    fn new(vars: &mut HashMap<Variable, (Arc<AtomicUsize>, Arc<AtomicBool>)>) -> Self {
        let element = VarPoint {
            x: VarValue::new(),
            y: VarValue::new(),
        };

        vars.insert(
            element.x.var,
            (element.x.value.clone(), element.x.has_changed.clone()),
        );
        vars.insert(
            element.y.var,
            (element.y.value.clone(), element.y.has_changed.clone()),
        );

        element
    }
}

impl std::fmt::Debug for VarPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{:?}: {}, {:?}: {}",
            self.x.var,
            self.x.value.load(Ordering::Relaxed),
            self.y.var,
            self.y.value.load(Ordering::Relaxed)
        ))
    }
}

/// A list of [`Constraint`] for [`Rect`]s to follow.
///
/// These [`Constraint`]s are specifically not related to a [`Rect`]s
/// location, in relation to other [`Rect`]s. They instead deal with
/// two things, affecting a [`Rect`]s lenght in its parent's [`Axis`]:
///
/// - `defined`: A [`Constraint`], provided by the user, which details
///   specific requests for the lenght of a [`Rect`].
/// - `ratio`: A [`Constraint`] which details the ratio between the
///   lenght of this [`Rect`] and the lenght of the [`Rect`] that
///   follows it, if there is any.
///
/// Both of these constraints are optional, and are meant to be
/// replaceable at runtime.
///
/// [`Constraint`]: CassowaryConstraint
#[derive(Default, Debug, Clone)]
struct Constraints {
    defined: Option<(CassowaryConstraint, Constraint)>,
    ratio: Option<(CassowaryConstraint, f64)>,
}

impl Constraints {
    /// Wether or not [`self`] has flexibility in terms of its lenght.
    fn is_resizable(&self) -> bool {
        matches!(
            self.defined,
            Some((_, Constraint::Min(_) | Constraint::Max(_))) | None
        )
    }
}

/// An area on the screen, which can hold other [`Rect`]s or be host
/// for printing a [`Widget`][parsec_core::widgets::Widget].
///
/// [`Rect`]s hold the [`Constraint`]s that define them, handling
/// which sides of the [`Rect`] should be equal to that of other
/// specific [`Rect`]s. These [`Constraint`]s also serve the role of
/// framing the [`Rect`] in cases where it is deemed necessary and
/// correct.
///
/// They can also be tied to other, ancestor [`Rect`]s, such that, by
/// moving the parent [`Rect`], you will be moving all of its children
/// along with it.
///
/// [`Constraint`]: CassowaryConstraint
#[derive(Debug)]
#[allow(clippy::type_complexity)]
pub struct Rect {
    index: AreaIndex,
    /// The index that this [`Rect`] is tied to.
    tl: VarPoint,
    br: VarPoint,
    edge_cons: Vec<CassowaryConstraint>,
    lineage: Option<(Vec<(RwData<Rect>, Constraints)>, Axis, bool)>,
}

impl Rect {
    /// Returns a new instance of [`Rect`], already adding its
    /// [`Variable`]s to the list.
    fn new(vars: &mut HashMap<Variable, (Arc<AtomicUsize>, Arc<AtomicBool>)>) -> Self {
        Rect {
            index: unique_area_index(),
            tl: VarPoint::new(vars),
            br: VarPoint::new(vars),
            edge_cons: Vec::new(),
            lineage: None,
        }
    }

    /// Returns a new [`Rect`], which is supposed to replace an
    /// existing [`Rect`], as its new parent.
    fn new_parent_of(
        rect: &mut Rect,
        axis: Axis,
        vars: &mut HashMap<Variable, (Arc<AtomicUsize>, Arc<AtomicBool>)>,
        new_glued: bool,
    ) -> Self {
        let parent = Rect {
            index: unique_area_index(),
            tl: rect.tl.clone(),
            br: rect.br.clone(),
            edge_cons: rect.edge_cons.clone(),
            lineage: Some((Vec::new(), axis, new_glued)),
        };

        rect.edge_cons.clear();
        rect.tl = VarPoint::new(vars);
        rect.br = VarPoint::new(vars);

        parent
    }

    /// Sets the [`CassowaryConstraint`]s for the edges of [`self`],
    /// taking into account the potential frame, defined by [`Frame`]
    /// and the [`Rect`]'s position.
    fn set_constraints(
        &mut self,
        parent: &Rect,
        axis: Axis,
        frame: Frame,
        max: Coord,
        edges: &mut Vec<Edge>,
    ) -> (f64, f64) {
        self.edge_cons.extend([
            self.tl.x.var | GE(REQUIRED) | 0.0,
            self.tl.y.var | GE(REQUIRED) | 0.0,
            self.br.x.var | GE(REQUIRED) | self.tl.x.var,
            self.br.y.var | GE(REQUIRED) | self.tl.y.var,
        ]);

        edges.retain(|edge| !edge.matches_vars(&self.br, &self.tl));
        let (right, up, left, down) = if self.is_frameable(Some(parent)) {
            self.form_frame(frame, max, edges)
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        let (para_left, para_right, start, end) = match axis.perp() {
            Axis::Vertical => (up, down, left, right),
            Axis::Horizontal => (left, right, up, down),
        };

        self.edge_cons.extend([
            self.start(axis.perp()) | EQ(REQUIRED) | (parent.start(axis.perp()) + para_left),
            (self.end(axis.perp()) + para_right) | EQ(REQUIRED) | parent.end(axis.perp()),
        ]);

        (start, end)
    }

    /// Sets the [`CassowaryConstraint`]s for the main [`Rect`], which
    /// is supposed to be parentless. It takes into account a possible
    /// [`Frame`].
    fn set_main_constraints(
        &mut self,
        frame: Frame,
        solver: &mut Solver,
        edges: &mut Vec<Edge>,
        max: Coord,
    ) {
        let (hor_edge, ver_edge) = if self.is_frameable(None) {
            frame.main_edges()
        } else {
            (0.0, 0.0)
        };

        edges.retain(|edge| !edge.matches_vars(&self.br, &self.tl));
        if hor_edge == 1.0 {
            edges.push(Edge::new(
                self.br.clone(),
                self.tl.clone(),
                Axis::Vertical,
                frame,
            ));
            edges.push(Edge::new(
                self.tl.clone(),
                self.br.clone(),
                Axis::Vertical,
                frame,
            ));
        }
        if ver_edge == 1.0 {
            edges.push(Edge::new(
                self.tl.clone(),
                self.br.clone(),
                Axis::Horizontal,
                frame,
            ));
            edges.push(Edge::new(
                self.br.clone(),
                self.tl.clone(),
                Axis::Horizontal,
                frame,
            ));
        }

        self.edge_cons = vec![
            self.tl.x.var | EQ(REQUIRED) | hor_edge,
            self.tl.y.var | EQ(REQUIRED) | ver_edge,
            self.br.x.var | EQ(REQUIRED) | (max.x as f64 - hor_edge),
            self.br.y.var | EQ(REQUIRED) | (max.y as f64 - ver_edge),
        ];

        solver.add_constraints(&self.edge_cons).unwrap();
    }

    /// Removes all [`CassowaryConstraint`]s which define the edges of
    /// [`self`].
    fn clear_constraints(&mut self, solver: &mut Solver) {
        for constraint in self.edge_cons.drain(..) {
            solver.remove_constraint(&constraint).unwrap();
        }
    }

    /// A [`Variable`], representing the "start" of [`self`], given an
    /// [`Axis`]. It will be the left or upper side of a [`Rect`].
    fn start(&self, axis: Axis) -> Variable {
        match axis {
            Axis::Horizontal => self.tl.x.var,
            Axis::Vertical => self.tl.y.var,
        }
    }

    /// A [`Variable`], representing the "start" of [`self`], given an
    /// [`Axis`]. It will be the right or lower side of a [`Rect`].
    fn end(&self, axis: Axis) -> Variable {
        match axis {
            Axis::Horizontal => self.br.x.var,
            Axis::Vertical => self.br.y.var,
        }
    }

    /// An [`Expression`] representing the lenght of [`self`] on a
    /// given [`Axis`].
    fn len(&self, axis: Axis) -> Expression {
        match axis {
            Axis::Horizontal => self.br.x.var - self.tl.x.var,
            Axis::Vertical => self.br.y.var - self.tl.y.var,
        }
    }

    /// The current value for the lenght of [`self`] on a given
    /// [`Axis`].
    fn len_value(&self, axis: Axis) -> usize {
        match axis {
            Axis::Horizontal => {
                self.br.x.value.load(Ordering::Relaxed) - self.tl.x.value.load(Ordering::Relaxed)
            }
            Axis::Vertical => {
                self.br.y.value.load(Ordering::Relaxed) - self.tl.y.value.load(Ordering::Relaxed)
            }
        }
    }

    /// The top left corner of [`self`].
    pub fn tl(&self) -> Coord {
        Coord {
            x: self.tl.x.value.load(Ordering::Acquire),
            y: self.tl.y.value.load(Ordering::Acquire),
        }
    }

    /// The bottom right corner of [`self`].
    pub fn br(&self) -> Coord {
        Coord {
            x: self.br.x.value.load(Ordering::Acquire),
            y: self.br.y.value.load(Ordering::Acquire),
        }
    }

    /// Wether or not [`self`] has children.
    pub fn is_parent(&self) -> bool {
        self.lineage.is_some()
    }

    /// Changes a child's defined constraint in [`Constraints`].
    pub fn change_child_constraint(
        &mut self,
        index: usize,
        defined: Constraint,
        solver: &mut Solver,
    ) -> bool {
        let (children, axis, _) = self.lineage.as_mut().unwrap();
        let axis = *axis;

        let (_, constraints) = &mut children[index];
        if !constraints
            .defined
            .as_ref()
            .is_some_and(|(_, cmp)| *cmp == defined)
        {
            if let Some((constraint, _)) = constraints.defined.take() {
                solver.remove_constraint(&constraint).unwrap();
            }
            set_defined_constraint(defined, self, index, axis, solver);

            true
        } else {
            false
        }
    }

    /// The index that identifies [`self`].
    pub fn index(&self) -> AreaIndex {
        self.index
    }

    pub fn has_changed(&self) -> bool {
        let br_x_changed = self.br.x.has_changed.swap(false, Ordering::Release);
        let br_y_changed = self.br.y.has_changed.swap(false, Ordering::Release);
        let tl_x_changed = self.tl.x.has_changed.swap(false, Ordering::Release);
        let tl_y_changed = self.tl.y.has_changed.swap(false, Ordering::Release);

        br_x_changed || br_y_changed || tl_x_changed || tl_y_changed
    }

    /// Wheter or not [`self`] can be framed.
    fn is_frameable(&self, parent: Option<&Rect>) -> bool {
        if parent.is_some_and(|parent| parent.lineage.as_ref().unwrap().2) {
            false
        } else if let Some((.., clustered)) = &self.lineage {
            *clustered
        } else {
            true
        }
    }

    /// Forms the frame surrounding [`self`], considering its position
    /// and a [`Frame`].
    fn form_frame(&self, frame: Frame, max: Coord, edges: &mut Vec<Edge>) -> (f64, f64, f64, f64) {
        let (right, up, left, down) = frame.edges(&self.br, &self.tl, max);

        if right == 1.0 {
            edges.push(Edge::new(
                self.br.clone(),
                self.tl.clone(),
                Axis::Vertical,
                frame,
            ));
        }
        if up == 1.0 {
            edges.push(Edge::new(
                self.tl.clone(),
                self.br.clone(),
                Axis::Horizontal,
                frame,
            ));
        }
        if left == 1.0 {
            edges.push(Edge::new(
                self.tl.clone(),
                self.br.clone(),
                Axis::Vertical,
                frame,
            ));
        }
        if down == 1.0 {
            edges.push(Edge::new(
                self.br.clone(),
                self.tl.clone(),
                Axis::Horizontal,
                frame,
            ));
        }

        (right, up, left, down)
    }
}

/// The overrall structure of a window on `parsec_term`.
///
/// The [`Layout`] handles all of the [`Rect`]s inside of it,
/// including all of the [`Variable`]s and
/// [`Constraint`][CassowaryConstraint]s that define said [`Rect`]s.
/// All external interactions seeking to change these values do so
/// through the [`Layout`].
///
/// The [`Layout`] also handles the [`Edge`]s that are supposed to be
/// printed to the screen.
///
/// The [`Layout`] will also hold floating [`Rect`]s, once those
/// become a thing.
pub struct Layout {
    vars: HashMap<Variable, (Arc<AtomicUsize>, Arc<AtomicBool>)>,
    main: RwData<Rect>,
    max: Coord,
    floating: Vec<RwData<Rect>>,
    frame: Frame,
    edges: Vec<Edge>,
    pub active_index: AreaIndex,
    pub solver: Solver,
}

impl Layout {
    /// Returns a new instance of [`Layout`], applying a given
    /// [`Frame`] to all inner [`Rect`]s.
    pub fn new(frame: Frame) -> Self {
        let max = {
            let (width, height) = crossterm::terminal::size().unwrap();
            Coord::new(width as usize, height as usize)
        };
        let mut edges = Vec::new();
        let mut solver = Solver::new();
        let mut vars = HashMap::new();

        let mut main = Rect::new(&mut vars);

        main.set_main_constraints(Frame::Empty, &mut solver, &mut edges, max);

        let active_index = main.index;
        let mut layout = Layout {
            vars,
            main: RwData::new(main),
            max,
            floating: Vec::new(),
            edges,
            frame,
            active_index,
            solver,
        };

        layout.update();

        layout.main.mutate(|main| {
            main.clear_constraints(&mut layout.solver);
            main.set_main_constraints(Frame::Empty, &mut layout.solver, &mut layout.edges, max);
        });

        layout.update();

        layout
    }

    /// The index of the main [`Rect`], which holds all (non floating)
    /// others.
    pub fn main_index(&self) -> AreaIndex {
        self.main.read().index
    }

    /// Fetches the [`RwData<Rect>`] of the given index, if there is
    /// one.
    pub fn fetch_index(&self, index: AreaIndex) -> Option<RwData<Rect>> {
        std::iter::once(&self.main)
            .chain(&self.floating)
            .find_map(|rect| fetch_index(rect, index))
    }

    /// Fetches the [`RwData<Rect>`] which holds the [`RwData<Rect>`]
    /// of the given index.
    ///
    /// Also returns the child's "child index", given an [`Axis`],
    /// going top to bottom or left to right.
    pub fn fetch_parent(&self, index: AreaIndex) -> Option<(RwData<Rect>, usize)> {
        std::iter::once(&self.main)
            .chain(&self.floating)
            .find_map(|rect| fetch_parent(rect, index))
    }

    /// Updates the value of all [`VarPoint`]s that have changed,
    /// returning true if any of them have.
    pub fn update(&mut self) {
        for (var, new) in self.solver.fetch_changes() {
            let (value, has_changed) = &self.vars[var];

            let new = new.round() as usize;
            let old = value.swap(new, Ordering::Release);
            has_changed.store(old != new, Ordering::Release);
        }
    }

    /// Bisects a given [`Rect`] into two [`Rect`]s, returning the
    /// index of the new one.
    ///
    /// This bisection will sometimes cause the creation of a new
    /// parent to hold the bisected [`Rect`] and its new sibling. In
    /// these cases, an aditional index associated with the parent
    /// will be returned.
    ///
    /// If `glue_new`, and the bisected [`Rect`] was "not glued", a
    /// new parent will be created, which will act as the holder for
    /// all of its successors. If this parent is moved, the children
    /// will move with it.
    ///
    /// If `glue_new`, and the bisected [`Rect`] was already glued,
    /// the creation of a new parent will follow regular rules, but
    /// the children will still be "glued".
    pub fn bisect(
        &mut self,
        index: AreaIndex,
        specs: PushSpecs,
        cluster_new: bool,
    ) -> (AreaIndex, Option<AreaIndex>) {
        let axis = Axis::from(specs);
        let (can_be_sibling, can_be_child) = {
            let parent_clustered = self
                .fetch_parent(index)
                .map(|(parent, ..)| parent.read().lineage.as_ref().unwrap().2)
                .unwrap_or(cluster_new);

            let child_clustered = self.fetch_index(index).unwrap().inspect(|child| {
                child
                    .lineage
                    .as_ref()
                    .is_some_and(|(.., clustered)| *clustered)
            });

            (
                parent_clustered == cluster_new,
                child_clustered == cluster_new,
            )
        };

        // Check for a parent of `self` with the same `Axis`.
        let (parent, index, new_parent_index) = if can_be_sibling
            && let Some((parent, index)) = self.fetch_parent(index).filter(|(parent, _)|
                parent.read().lineage.as_ref().unwrap().1 == axis
            )
        {
            let index = match specs.comes_earlier() {
                true => index,
                false => index + 1
            };

            (parent, index, None)
        // Checking if `self` is a parent with the same `Axis`.
        } else if can_be_child
        	&& let Some(parent) = self
        		.fetch_index(index)
        		.filter(|area| area.read().lineage.as_ref().is_some_and(|(_, cmp, _)| axis == *cmp))
        {
            let index = match specs.comes_earlier() {
                true => 0,
                false => parent.read().lineage.as_ref().unwrap().0.len()
            };

            (parent, index, None)
        // If all else fails, create a new parent to hold both `self`
        // and the new area.
        } else {
            let rect = self.fetch_index(index).unwrap();
            let child_is_main = rect.ptr_eq(&self.main);

            let (new_parent_index, child) = rect.mutate(|rect| {
                let parent = Rect::new_parent_of(rect, axis, &mut self.vars, cluster_new);
                (parent.index, std::mem::replace(rect, parent))
            });

            let index = match specs.comes_earlier() {
                true => 0,
                false => 1
            };

			let defined = if let Some((grandpa, index)) = self.fetch_parent(new_parent_index) {
    			let mut grandpa = grandpa.write();
    			let (children, axis, _) = grandpa.lineage.as_mut().unwrap();
    			let constraints = std::mem::take(&mut children[index].1);
    			constraints.defined.map(|(constraint, defined)| {
        			self.solver.remove_constraint(&constraint).unwrap();
        			(defined, *axis)
    			})
			} else {
    			None
			};

            rect.mutate(|parent| {
                parent.lineage = Some(
                    (vec![(RwData::new(child), Constraints::default())], axis, cluster_new)
                );

    			if let Some((defined, axis)) = defined {
        			set_defined_constraint(defined, parent, 0, axis, &mut self.solver);
    			}
            });

            // If the child is glued, the frame doesn't need to be redone.
            if !cluster_new {
                if child_is_main {
                    let (solver, edges) = (&mut self.solver, &mut self.edges);
                    let mut rect = rect.write();
                    rect.clear_constraints(solver);
                    rect.set_main_constraints(self.frame, solver, edges, self.max);
                } else {
                    let (parent, index) = self.fetch_parent(rect.read().index).unwrap();
                    let parent = parent.write();
                    let (solver, edges) = (&mut self.solver, &mut self.edges);
                    prepare_child(&parent, index, solver, self.frame, self.max, edges)
                }
            }

            (rect, index, Some(new_parent_index))
        };

        self.update();

        let (temp_constraint, new_index) = parent.mutate(|parent| {
            let new = Rect::new(&mut self.vars);
            let new_index = new.index;

            let (len, axis) = parent
                .lineage
                .as_mut()
                .map(|(children, axis, _)| {
                    children.insert(index, (RwData::new(new), Constraints::default()));
                    (children.len(), *axis)
                })
                .unwrap();

            if let Some(defined) = specs.constraint() {
                set_defined_constraint(defined, parent, index, axis, &mut self.solver);
            }

            // We initially set the frame to `Frame::Empty`, since that will make
            // the `Rect`s take their "full space". What this means is that, when
            // we calculate the real frame, using `self.frame`, we know for a fact
            // which `Rect`s reach the edge of the screen. Unfortunately, this
            // does mean that we have to run `prepare_child()` twice for each
            // affected area.
            let edges = &mut self.edges;
            let next = (len - 1).checked_sub(index + 1).map(|_| index + 1);
            for &index in [index.checked_sub(1), Some(index), next].iter().flatten() {
                prepare_child(
                    parent,
                    index,
                    &mut self.solver,
                    Frame::Empty,
                    self.max,
                    edges,
                );
            }

            // Add a constraint so that the new child `Rect` has a len equal to
            // `resizable_len / resizable_children`, a self imposed rule.
            // This will be useful later, when adding new ratio constraints to the
            // new child.
            let constraint = specs.constraint();
            if let Some(Constraint::Min(_) | Constraint::Max(_)) | None = constraint {
                let (children, axis, _) = parent.lineage.as_ref().unwrap();
                let (res_count, res_len) = children
                    .iter()
                    .filter(|(_, constraints)| constraints.is_resizable())
                    .fold((0, 0), |(count, len), (child, _)| {
                        (count + 1, len + child.read().len_value(*axis))
                    });

                let temp_constraint = {
                    let (new, _) = children[index].clone();
                    let new = new.read();
                    new.len(*axis) | EQ(WEAK * 2.0) | (res_len as f64 / res_count as f64)
                };

                self.solver.add_constraint(temp_constraint.clone()).unwrap();

                (Some(temp_constraint), new_index)
            } else {
                (None, new_index)
            }
        });

        self.update();

        parent.mutate(|parent| {
            if let Some(Constraint::Min(_) | Constraint::Max(_)) | None = specs.constraint() {
                set_ratio_constraints(parent, index, &mut self.solver);
            }

            if let Some(temp_constraint) = temp_constraint {
                self.solver.remove_constraint(&temp_constraint).unwrap();
            }
        });

        self.update();

        parent.mutate(|parent| {
            let len = parent.lineage.as_ref().unwrap().0.len();
            let edges = &mut self.edges;
            // Second frame calculation, this time, using the real `self.frame`.
            let next = (len - 1).checked_sub(index + 1).map(|_| index + 1);
            for &index in [index.checked_sub(1), Some(index), next].iter().flatten() {
                prepare_child(parent, index, &mut self.solver, self.frame, self.max, edges);
            }
        });

        self.update();

        (new_index, new_parent_index)
    }

    /// The current value for the width of [`self`].
    pub fn width(&self) -> usize {
        self.main.read().len_value(Axis::Horizontal)
    }

    /// The current value for the height of [`self`].
    pub fn height(&self) -> usize {
        self.main.read().len_value(Axis::Vertical)
    }

    /// The [`Edge`]s that are supposed to be printed to the screen.
    pub fn edges(&self) -> &[Edge] {
        self.edges.as_ref()
    }
}

/// Assigns all [`CassowaryConstraint`]s that are appropriate to a
/// given child, with the exception of defined and ratio constraints.
fn prepare_child(
    parent: &Rect,
    index: usize,
    solver: &mut Solver,
    frame: Frame,
    max: Coord,
    edges: &mut Vec<Edge>,
) {
    let (children, axis, _) = parent.lineage.as_ref().unwrap();
    let mut child = children[index].0.write();

    child.clear_constraints(solver);
    let (start, end) = child.set_constraints(parent, *axis, frame, max, edges);

    if index == 0 {
        let constraint = child.start(*axis) | EQ(REQUIRED) | (parent.start(*axis) + start);
        child.edge_cons.push(constraint);
    }

    // Previous children carry the `Constraint`s for the `start` of their
    // successors.
    let constraint = if let Some((next, _)) = children.get(index + 1) {
        (child.end(*axis) + end) | EQ(REQUIRED) | next.read().start(*axis)
    } else {
        (child.end(*axis) + end) | EQ(REQUIRED) | parent.end(*axis)
    };
    child.edge_cons.push(constraint);

    solver.add_constraints(&child.edge_cons).unwrap();

    if let Some((children, ..)) = &mut child.lineage {
        let len = children.len();
        for index in 0..len {
            prepare_child(&child, index, solver, frame, max, edges);
        }
    }
}

/// Sets the ratio [`CassowaryConstraint`]s for the child of the given
/// index. This does include setting the [`CassowaryConstraint`] for
/// the previous child, if there is one.
fn set_ratio_constraints(parent: &mut Rect, index: usize, solver: &mut Solver) {
    let &(_, axis, _) = parent.lineage.as_ref().unwrap();

    let (children, ..) = parent.lineage.as_mut().unwrap();

    let (new, _) = children[index].clone();
    let new = new.read();

    let prev = children.iter().take(index);
    let res_index = prev
        .filter(|(_, constraints)| constraints.is_resizable())
        .count();

    let mut children = children
        .iter_mut()
        .filter(|(_, constraints)| constraints.is_resizable());

    if res_index > 0 {
        let (prev, constraints) = children.nth(res_index - 1).unwrap();

        let prev = prev.read();
        let ratio = if new.len_value(axis) == 0 {
            1.0
        } else {
            prev.len_value(axis) as f64 / new.len_value(axis) as f64
        };

        let constraint = prev.len(axis) | EQ(WEAK) | (ratio * new.len(axis));
        constraints.ratio = Some((constraint.clone(), ratio));
        solver.add_constraint(constraint).unwrap();
    }

    let ratio = children.nth(1).map(|(next, _)| {
        let next = next.read();
        let ratio = if next.len_value(axis) == 0 {
            1.0
        } else {
            new.len_value(axis) as f64 / next.len_value(axis) as f64
        };

        let constraint = new.len(axis) | EQ(WEAK) | (ratio * next.len(axis));
        solver.add_constraint(constraint.clone()).unwrap();

        (constraint, ratio)
    });

    parent.lineage.as_mut().unwrap().0[index].1.ratio = ratio;
}

/// Returns a [`CassowaryConstraint`] representing the defined
/// [`Constraint`].
fn set_defined_constraint(
    defined: Constraint,
    parent: &mut Rect,
    index: usize,
    axis: Axis,
    solver: &mut Solver,
) {
    let parent_len = parent.len(axis);

    let (children, ..) = parent.lineage.as_mut().unwrap();
    let (child, constraints) = &mut children[index];

    let child = child.read();
    let constraint = match defined {
        Constraint::Ratio(den, div) => {
            assert!(den < div, "Constraint::Ratio must be smaller than 1.");
            child.len(axis) | EQ(WEAK * 2.0) | (parent_len * (den as f64 / div as f64))
        }
        Constraint::Percent(percent) => {
            assert!(
                percent <= 100,
                "Constraint::Percent must be smaller than 100"
            );
            child.len(axis) | EQ(WEAK * 2.0) | (parent_len * (percent as f64 / 100.0))
        }
        Constraint::Length(len) => child.len(axis) | EQ(STRONG) | len,
        Constraint::Min(min) => child.len(axis) | GE(MEDIUM) | min,
        Constraint::Max(max) => child.len(axis) | LE(MEDIUM) | max,
    };

    solver.add_constraint(constraint.clone()).unwrap();

    constraints.defined = Some((constraint, defined));
}

/// Fetches the [`RwData<Rect>`] with the given index.
fn fetch_index(rect: &RwData<Rect>, index: AreaIndex) -> Option<RwData<Rect>> {
    if rect.read().index == index {
        Some(rect.clone())
    } else {
        rect.inspect(|rect| {
            rect.lineage.as_ref().and_then(|(children, ..)| {
                children
                    .iter()
                    .find_map(|(child, _)| fetch_index(child, index))
            })
        })
    }
}

/// Fetches the parent of the [`RwData<Rect>`] with the given index,
/// including its positional index and the [`Axis`] of its children.
fn fetch_parent(main: &RwData<Rect>, index: AreaIndex) -> Option<(RwData<Rect>, usize)> {
    let rect = main.read();

    if rect.index == index {
        return None;
    }
    let Some((children, ..)) = &rect.lineage else {
        return None;
    };

    children.iter().enumerate().find_map(|(pos, (child, _))| {
        if child.read().index == index {
            Some((main.clone(), pos))
        } else {
            fetch_parent(child, index)
        }
    })
}
