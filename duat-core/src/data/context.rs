use std::{
    any::TypeId,
    sync::{
        LazyLock,
        atomic::{AtomicUsize, Ordering},
    },
};

pub use self::global::*;
use super::{RoData, RwData, private::InnerData};
use crate::{
    mode::{self, Cursors},
    ui::{Area, Ui},
    widgets::{File, Node, Widget},
};

mod global {
    use std::{
        any::Any,
        sync::{
            LazyLock, OnceLock,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use super::{CurFile, CurWidget, FileParts, FileReader};
    use crate::{
        Error, Result,
        data::RwData,
        duat_name,
        mode::Regular,
        text::Text,
        ui::{Ui, Window},
        widgets::{File, Node},
    };

    static MODE_NAME: LazyLock<RwData<&str>> =
        LazyLock::new(|| RwData::new(duat_name::<Regular>()));
    static CUR_FILE: OnceLock<&(dyn Any + Send + Sync)> = OnceLock::new();
    static CUR_WIDGET: OnceLock<&(dyn Any + Send + Sync)> = OnceLock::new();
    static CUR_WINDOW: AtomicUsize = AtomicUsize::new(0);
    static WINDOWS: OnceLock<&(dyn Any + Send + Sync)> = OnceLock::new();
    static NOTIFICATIONS: LazyLock<RwData<Text>> = LazyLock::new(RwData::default);

    pub fn mode_name() -> &'static RwData<&'static str> {
        &MODE_NAME
    }

    pub fn fixed_reader<U: Ui>() -> Result<FileReader<U>, File> {
        Ok(cur_file()?.fixed_reader())
    }

    pub fn dyn_reader<U: Ui>() -> Result<FileReader<U>, File> {
        Ok(cur_file()?.dyn_reader())
    }

    pub fn cur_file<U: Ui>() -> Result<&'static CurFile<U>, File> {
        let cur_file = inner_cur_file();
        cur_file.0.read().as_ref().ok_or(Error::NoFileYet)?;
        Ok(cur_file)
    }

    pub fn cur_widget<U: Ui>() -> Result<&'static CurWidget<U>, File> {
        let cur_widget = inner_cur_widget();
        cur_widget.0.read().as_ref().ok_or(Error::NoWidgetYet)?;
        Ok(cur_widget)
    }

    pub fn cur_window() -> usize {
        CUR_WINDOW.load(Ordering::Relaxed)
    }

    pub fn notifications() -> &'static RwData<Text> {
        &NOTIFICATIONS
    }

    pub fn notify(msg: Text) {
        *NOTIFICATIONS.write() = msg
    }

    pub fn setup<U: Ui>(
        cur_file: &'static CurFile<U>,
        cur_widget: &'static CurWidget<U>,
        cur_window: usize,
        windows: &'static RwData<Vec<Window<U>>>,
    ) {
        CUR_FILE.set(cur_file).expect("setup ran twice");
        CUR_WIDGET.set(cur_widget).expect("setup ran twice");
        CUR_WINDOW.store(cur_window, Ordering::Relaxed);
        WINDOWS.set(windows).expect("setup ran twice");
    }

    pub(crate) fn set_cur<U: Ui>(
        parts: Option<FileParts<U>>,
        node: Node<U>,
    ) -> Option<(FileParts<U>, Node<U>)> {
        let prev = parts.and_then(|p| inner_cur_file().0.write().replace(p));

        prev.zip(inner_cur_widget().0.write().replace(node))
    }

    pub(crate) fn set_windows<U: Ui>(win: Vec<Window<U>>) -> &'static AtomicUsize {
        *windows().write() = win;
        &CUR_WINDOW
    }

    pub(crate) fn windows<U: Ui>() -> &'static RwData<Vec<Window<U>>> {
        WINDOWS.get().unwrap().downcast_ref().expect("1 Ui only")
    }

    pub(crate) fn inner_cur_file<U: Ui>() -> &'static CurFile<U> {
        CUR_FILE.get().unwrap().downcast_ref().expect("1 Ui only")
    }

    pub(crate) fn inner_cur_widget<U: Ui>() -> &'static CurWidget<U> {
        CUR_WIDGET.get().unwrap().downcast_ref().expect("1 Ui only")
    }
}

pub struct CurFile<U: Ui>(LazyLock<RwData<Option<FileParts<U>>>>);

impl<U: Ui> CurFile<U> {
    pub const fn new() -> Self {
        Self(LazyLock::new(|| RwData::new(None)))
    }

    pub fn fixed_reader(&self) -> FileReader<U> {
        let data = self.0.raw_read();
        let (file, area, cursors, related) = data.clone().unwrap();
        let file_state = AtomicUsize::new(file.cur_state().load(Ordering::Relaxed));
        let cursors_state = AtomicUsize::new(cursors.cur_state.load(Ordering::Relaxed));

        FileReader {
            data: RoData::new(Some((file, area, cursors, related))),
            file_state,
            cursors_state,
        }
    }

    pub fn dyn_reader(&self) -> FileReader<U> {
        let data = self.0.raw_read();
        let (file, .., cursors, _) = data.clone().unwrap();

        FileReader {
            data: RoData::from(&*self.0),
            file_state: AtomicUsize::new(file.cur_state.load(Ordering::Relaxed)),
            cursors_state: AtomicUsize::new(cursors.cur_state.load(Ordering::Relaxed)),
        }
    }

    pub fn inspect<R>(&self, f: impl FnOnce(&File, &U::Area, &Cursors) -> R) -> R {
        let data = self.0.raw_read();
        let (file, area, cursors, _) = data.as_ref().unwrap();

        cursors.inspect(|c| f(&file.read(), area, c))
    }

    /// The name of the active [`File`]'s file.
    pub fn name(&self) -> String {
        self.0.raw_read().as_ref().unwrap().0.read().name()
    }

    /// The name of the active [`File`]'s file.
    pub fn path(&self) -> String {
        self.0.raw_read().as_ref().unwrap().0.read().path()
    }

    // NOTE: Doesn't return result, since it is expected that widgets can
    // only be created after the file exists.
    pub fn file_ptr_eq(&self, other: &Node<U>) -> bool {
        other.ptr_eq(&self.0.read().as_ref().unwrap().0)
    }

    pub(crate) fn mutate_data<R>(
        &self,
        f: impl FnOnce(&RwData<File>, &U::Area, &RwData<Cursors>) -> R,
    ) -> R {
        let data = self.0.raw_read();
        let (file, area, cursors, _) = data.as_ref().unwrap();

        cursors.inspect(|c| {
            let mut file = file.write();
            let cfg = <File as Widget<U>>::print_cfg(&file);
            <File as Widget<U>>::text_mut(&mut file).remove_cursors(c, area, cfg);
        });

        let ret = f(file, area, cursors);

        let cursors = cursors.read();

        let mut file = file.write();
        let cfg = <File as Widget<U>>::print_cfg(&file);

        if let Some(main) = cursors.get_main() {
            area.scroll_around_point(
                file.text(),
                main.caret(),
                <File as Widget<U>>::print_cfg(&file),
            );
        }
        file.text_mut().add_cursors(&cursors, area, cfg);

        <File as Widget<U>>::update(&mut file, area);
        if !mode::is_printing_stopped() {
            <File as Widget<U>>::print(&mut file, area);
        }

        ret
    }

    pub(crate) fn mutate_related_widget<W: Widget<U>, R>(
        &self,
        f: impl FnOnce(&mut W, &U::Area, &mut Cursors) -> R,
    ) -> Option<R> {
        let f = move |w: &mut W, a, c: &mut Cursors| {
            let cfg = w.print_cfg();
            w.text_mut().remove_cursors(c, a, cfg);

            let ret = f(w, a, &mut *c);

            let cfg = w.print_cfg();

            if let Some(main) = c.get_main() {
                a.scroll_around_point(w.text(), main.caret(), w.print_cfg());
            }
            w.text_mut().add_cursors(c, a, cfg);
            w.update(a);
            if !mode::is_printing_stopped() {
                w.print(a);
            }

            ret
        };

        let data = self.0.raw_read();
        let (file, area, cursors, rel) = data.as_ref().unwrap();

        let rel = rel.read();
        if file.data_is::<W>() {
            let mut cursors = cursors.write();
            file.mutate_as(|w| f(w, area, &mut cursors))
        } else {
            rel.iter()
                .find(|node| node.data_is::<W>())
                .and_then(|node| {
                    let (widget, area, cursors) = node.as_active();
                    let mut cursors = cursors.write();
                    widget.mutate_as(|w| f(w, area, &mut cursors))
                })
        }
    }

    pub(crate) fn get_related_widget<W: Widget<U>>(&self) -> Option<Node<U>> {
        let data = self.0.write();
        let (.., related) = data.as_ref().unwrap();
        let related = related.read();

        related.iter().find(|node| node.data_is::<W>()).cloned()
    }
}

impl<U: Ui> Default for CurFile<U> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FileReader<U: Ui> {
    data: RoData<Option<FileParts<U>>>,
    file_state: AtomicUsize,
    cursors_state: AtomicUsize,
}

impl<U: Ui> FileReader<U> {
    pub fn inspect<R>(&self, f: impl FnOnce(&File, &U::Area, &Cursors) -> R) -> R {
        let data = self.data.read();
        let (file, area, cursors, _) = data.as_ref().unwrap();

        self.file_state
            .store(file.cur_state().load(Ordering::Acquire), Ordering::Release);
        self.cursors_state.store(
            cursors.cur_state().load(Ordering::Acquire),
            Ordering::Release,
        );

        let cursors = cursors.read();
        let file = file.read();
        f(&file, area, &cursors)
    }

    pub fn inspect_related<T: 'static, R>(&self, f: impl FnOnce(&T) -> R) -> Option<R> {
        let data = self.data.read();
        let (file, _, cursors, related) = data.as_ref().unwrap();

        if file.data_is::<T>() {
            file.inspect_as(f)
        } else if cursors.data_is::<T>() {
            cursors.inspect_as(f)
        } else {
            let related = related.read();
            related
                .iter()
                .find(|node| node.data_is::<T>())
                .and_then(|w| w.inspect_as(f))
        }
    }

    pub fn inspect_file_and<T: 'static, R>(&self, f: impl FnOnce(&File, &T) -> R) -> Option<R> {
        let data = self.data.read();
        let (file, _, cursors, related) = data.as_ref().unwrap();

        if cursors.data_is::<T>() {
            cursors.inspect_as(|c| f(&file.read(), c))
        } else if cursors.data_is::<T>() {
            cursors.inspect_as::<T, R>(|c| f(&file.read(), c))
        } else {
            let related = related.read();
            related
                .iter()
                .find(|node| node.data_is::<T>())
                .and_then(|node| node.inspect_as(|widget| f(&file.read(), widget)))
        }
    }

    /// The name of the active [`File`]'s file.
    pub fn name(&self) -> String {
        self.data.read().as_ref().unwrap().0.read().name()
    }

    pub fn has_swapped(&self) -> bool {
        self.data.has_changed()
    }

    pub fn has_changed(&self) -> bool {
        // In the case where the active file has changed, this function will
        // return true, while also making sure that the `_state` fields point
        // to the new file.
        let mut has_changed = self.data.has_changed();
        let data = self.data.read();
        let (file, .., cursors, _) = data.as_ref().unwrap();

        has_changed |= {
            let state = file.cur_state().load(Ordering::Acquire);
            state > self.file_state.swap(state, Ordering::Acquire)
        };
        has_changed |= {
            let state = cursors.cur_state().load(Ordering::Acquire);
            state > self.cursors_state.swap(state, Ordering::Acquire)
        };

        has_changed
    }
}

impl<U: Ui> Clone for FileReader<U> {
    fn clone(&self) -> Self {
        let (file, .., cursors, _) = self.data.read().clone().unwrap();

        Self {
            data: self.data.clone(),
            file_state: AtomicUsize::new(file.cur_state().load(Ordering::Relaxed)),
            cursors_state: AtomicUsize::new(cursors.cur_state().load(Ordering::Relaxed)),
        }
    }
}

pub struct CurWidget<U: Ui>(LazyLock<RwData<Option<Node<U>>>>);

impl<U: Ui> CurWidget<U> {
    pub const fn new() -> Self {
        Self(LazyLock::new(|| RwData::new(None)))
    }

    pub fn type_id(&self) -> TypeId {
        self.0.type_id
    }

    pub fn inspect<R>(&self, f: impl FnOnce(&dyn Widget<U>, &U::Area, &Cursors) -> R) -> R {
        let data = self.0.raw_read();
        let (widget, area, cursors) = data.as_ref().unwrap().as_active();
        let cursors = cursors.read();
        let widget = widget.read();

        f(&*widget, area, &cursors)
    }

    pub fn inspect_widget_as<W, R>(&self, f: impl FnOnce(&W, &U::Area, &Cursors) -> R) -> Option<R>
    where
        W: Widget<U>,
    {
        let data = self.0.raw_read();
        let (widget, area, cursors) = data.as_ref().unwrap().as_active();
        let cursors = cursors.read();

        widget.inspect_as::<W, R>(|widget| f(widget, area, &cursors))
    }

    pub fn inspect_as<W: Widget<U>, R>(
        &self,
        f: impl FnOnce(&W, &U::Area, &Cursors) -> R,
    ) -> Option<R> {
        let data = self.0.raw_read();
        let (widget, area, cursors) = data.as_ref().unwrap().as_active();
        let cursors = cursors.read();

        widget.inspect_as(|w| f(w, area, &cursors))
    }

    pub(crate) fn mutate_data<R>(
        &self,
        f: impl FnOnce(&RwData<dyn Widget<U>>, &U::Area, &RwData<Cursors>) -> R,
    ) -> R {
        let data = self.0.read();
        let (widget, area, cursors) = data.as_ref().unwrap().as_active();

        cursors.inspect(|c| {
            let mut widget = widget.raw_write();
            let cfg = widget.print_cfg();
            widget.text_mut().remove_cursors(c, area, cfg)
        });

        let ret = f(widget, area, cursors);

        cursors.inspect(|c| {
            let mut widget = widget.write();
            let cfg = widget.print_cfg();

            if let Some(main) = c.get_main() {
                area.scroll_around_point(widget.text(), main.caret(), widget.print_cfg());
            }
            widget.text_mut().add_cursors(c, area, cfg);

            widget.update(area);
            if !mode::is_printing_stopped() {
                widget.print(area);
            }
        });

        ret
    }

    pub(crate) fn mutate_data_as<W: Widget<U>, R>(
        &self,
        f: impl FnOnce(&RwData<W>, &U::Area, &RwData<Cursors>) -> R,
    ) -> Option<R> {
        let data = self.0.read();
        let (widget, area, cursors) = data.as_ref().unwrap().as_active();

        let widget = widget.try_downcast::<W>()?;

        cursors.inspect(|c| {
            let mut widget = widget.raw_write();
            let cfg = widget.print_cfg();
            widget.text_mut().remove_cursors(c, area, cfg)
        });

        let ret = Some(f(&widget, area, cursors));

        cursors.inspect(|c| {
            let mut widget = widget.write();
            let cfg = widget.print_cfg();

            if let Some(main) = c.get_main() {
                area.scroll_around_point(widget.text(), main.caret(), widget.print_cfg());
            }
            widget.text_mut().add_cursors(c, area, cfg);

            widget.update(area);
            if !mode::is_printing_stopped() {
                widget.print(area);
            }
        });

        ret
    }

    pub(crate) fn node(&self) -> Node<U> {
        self.0.read().as_ref().unwrap().clone()
    }
}

impl<U: Ui> Default for CurWidget<U> {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) type FileParts<U> = (
    RwData<File>,
    <U as Ui>::Area,
    RwData<Cursors>,
    RwData<Vec<Node<U>>>,
);
