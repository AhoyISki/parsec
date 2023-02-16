use std::{
    cmp::min,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    action::{History, TextRange},
    config::RwData,
    cursor::{Editor, Mover, SpliceAdder, TextCursor, TextPos},
    max_line,
    tags::MatchManager,
    text::{reader::MutTextReader, update_range, PrintInfo, Text},
    ui::{Area, EndNode, Label, MidNode, Ui},
};

use super::{ActionableWidget, NormalWidget, Widget};

/// The widget that is used to print and edit files.
pub struct FileWidget<U>
where
    U: Ui,
{
    end_node: RwData<EndNode<U>>,
    pub(crate) mid_node: Option<RwData<MidNode<U>>>,
    pub(crate) side_widgets: Vec<(Widget<U>, usize)>,
    name: RwData<String>,
    text: Text<U>,
    print_info: PrintInfo,
    main_cursor: usize,
    cursors: Vec<TextCursor>,
    history: History,
    readers: Vec<Box<dyn MutTextReader<U>>>,
}

impl<U> FileWidget<U>
where
    U: Ui,
{
    /// Returns a new instance of `FileWidget`.
    pub fn new(
        path: &PathBuf, node: RwData<EndNode<U>>, match_manager: &Option<MatchManager>,
    ) -> Self {
        // TODO: Allow the creation of a new file.
        let file_contents = fs::read_to_string(path).expect("Failed to read the file.");
        let text = Text::new(file_contents, match_manager.clone());
        let cursor = TextCursor::new(TextPos::default(), text.lines(), &node.read());

        let file_widget = FileWidget {
            end_node: node,
            mid_node: None,
            side_widgets: Vec::new(),
            name: RwData::new(path.file_name().unwrap().to_string_lossy().to_string()),
            text,
            print_info: PrintInfo::default(),
            main_cursor: 0,
            cursors: vec![cursor],
            history: History::new(),
            readers: Vec::new(),
        };

        file_widget
    }

    /// Tbh, I don't remember what this is supposed to do, but it seems important.
    fn _match_scroll(&mut self) {
        let node = self.end_node.read();
        let label = node.label.read();

        let main_cursor = self.main_cursor();
        let limit_row =
            min(main_cursor.caret().row + label.area().height(), self.text.lines().len() - 1);
        let start = main_cursor.caret().translate(self.text.lines(), limit_row, 0);
        let target_line = &self.text.lines()[limit_row];
        let _range = TextRange {
            start,
            end: TextPos {
                byte: start.byte + target_line.text().len(),
                col: target_line.char_count(),
                ..start
            },
        };
    }

    /// Undoes the last moment in history.
    pub fn undo(&mut self) {
        let end_node = self.end_node.read();

        let moment = match self.history.move_backwards() {
            Some(moment) => moment,
            None => return,
        };

        self.print_info = moment.starting_print_info;

        self.cursors.clear();

        let mut splice_adder = SpliceAdder::default();
        for change in &moment.changes {
            let mut splice = change.splice;

            splice.calibrate_on_adder(&splice_adder);
            splice_adder.reset_cols(&splice.added_end);

            self.text.undo_change(&change, &splice);

            splice_adder.calibrate(&splice.reverse());

            self.cursors.push(TextCursor::new(splice.taken_end(), &self.text.lines, &end_node));

            let range = TextRange { start: splice.start(), end: splice.taken_end() };
            let max_line = max_line(&self.text, &self.print_info, &self.end_node.read());
            update_range(&mut self.text, range, max_line, &self.end_node.read());
        }
    }

    /// Redoes the last moment in history.
    pub fn redo(&mut self) {
        let end_node = self.end_node.read();

        let moment = match self.history.move_forward() {
            Some(moment) => moment,
            None => return,
        };

        self.print_info = moment.ending_print_info;

        self.cursors.clear();

        for change in &moment.changes {
            self.text.apply_change(&change);

            let splice = change.splice;

            self.cursors.push(TextCursor::new(splice.added_end(), &self.text.lines, &end_node));

            let range = TextRange { start: splice.start(), end: splice.added_end() };
            let max_line = max_line(&self.text, &self.print_info, &self.end_node.read());
            update_range(&mut self.text, range, max_line, &self.end_node.read());
        }
    }

    /// Returns the currently printed set of lines.
    pub fn printed_lines(&self) -> Vec<usize> {
        let end_node = self.end_node.read();
        let label = end_node.label.read();
        let height = label.area().height();
        let mut lines_iter = self.text.lines().iter().enumerate();
        let mut printed_lines = Vec::with_capacity(label.area().height());

        let top_line = lines_iter.nth(self.print_info.top_row).unwrap().1;
        let mut d_y = min(height, 1 + top_line.iter_wraps().count() - self.print_info.top_wraps);
        for _ in 0..d_y {
            printed_lines.push(self.print_info.top_row);
        }

        while let (Some((index, line)), true) = (lines_iter.next(), d_y < height) {
            let old_d_y = d_y;
            d_y = min(d_y + line.iter_wraps().count(), height);
            for _ in old_d_y..=d_y {
                printed_lines.push(index);
            }
        }

        printed_lines
    }

    // TODO: Move the history to a general placement, taking in all the files.
    /// The history associated with this file.
    pub fn history(&self) -> &History {
        &self.history
    }

    /// Ends the current moment and starts a new one.
    pub fn new_moment(&mut self) {
        self.cursors.iter_mut().for_each(|cursor| cursor.assoc_index = None);
        self.history.new_moment(self.print_info);
    }

    /// The list of `TextCursor`s on the file.
    pub fn cursors(&self) -> Vec<TextCursor> {
        self.cursors.clone()
    }

    /// The `MidNode` associated with this `FileWidget`.
    pub fn mid_node(&self) -> &Option<RwData<MidNode<U>>> {
        &self.mid_node
    }

    /// A mutable reference to the `Text` of self.
    pub fn mut_text(&mut self) -> &mut Text<U> {
        &mut self.text
    }

    ////////// Status line convenience functions:
    /// The main cursor of the file.
    pub fn main_cursor(&self) -> TextCursor {
        *self.cursors.get(self.main_cursor).unwrap()
    }

    /// The file's name.
    pub fn name(&self) -> String {
        self.name.read().clone()
    }

    pub fn full_path(&self) -> String {
        let mut path = std::env::current_dir().unwrap();
        path.push(Path::new(&self.name.read().as_str()));
        path.to_string_lossy().to_string()
    }

    /// The lenght of the file, in lines.
    pub fn len(&self) -> usize {
        self.text.lines().len()
    }

    /// The `PrintInfo` of the `FileWidget`.
    pub fn print_info(&self) -> PrintInfo {
        self.print_info
    }

    pub fn add_reader(&mut self, reader: Box<dyn MutTextReader<U>>) {
        self.readers.push(reader);
    }
}

impl<U> NormalWidget<U> for FileWidget<U>
where
    U: Ui,
{
    fn identifier(&self) -> String {
        String::from("file")
    }

    fn end_node(&self) -> &RwData<EndNode<U>> {
        &self.end_node
    }

    fn end_node_mut(&mut self) -> &mut RwData<EndNode<U>> {
        &mut self.end_node
    }

    fn update(&mut self) {
        self.print_info.update(self.main_cursor().caret(), &self.text, &self.end_node);
        panic!();

        //let mut node = self.end_node.write();
        //self.text.update_lines(&mut node);
        //drop(node);
        //self.match_scroll();
    }

    fn needs_update(&self) -> bool {
        true
    }

    fn text(&self) -> &Text<U> {
        &self.text
    }

    fn members_for_printing(&mut self) -> (&Text<U>, &mut RwData<EndNode<U>>, PrintInfo) {
        (&self.text, &mut self.end_node, self.print_info)
    }

    fn scroll_vertically(&mut self, d_y: i32) {
        self.print_info.scroll_vertically(d_y, &self.text);
    }
}

impl<U> ActionableWidget<U> for FileWidget<U>
where
    U: Ui,
{
    fn editor<'a>(&'a mut self, index: usize, splice_adder: &'a mut SpliceAdder) -> Editor<U> {
        Editor::new(
            &mut self.cursors[index],
            splice_adder,
            &mut self.text,
            &self.end_node,
            Some(&mut self.history),
            Some(self.print_info),
        )
    }

    fn mover(&mut self, index: usize) -> Mover<U> {
        Mover::new(
            &mut self.cursors[index],
            &self.text,
            &self.end_node,
            self.history.current_moment(),
        )
    }

    fn members_for_cursor_tags(&mut self) -> (&mut Text<U>, &[TextCursor], usize) {
        (&mut self.text, self.cursors.as_slice(), self.main_cursor)
    }

    fn cursors(&self) -> &[TextCursor] {
        self.cursors.as_slice()
    }

    fn mut_cursors(&mut self) -> Option<&mut Vec<TextCursor>> {
        Some(&mut self.cursors)
    }

    fn main_cursor_index(&self) -> usize {
        self.main_cursor
    }

    fn mut_main_cursor_index(&mut self) -> Option<&mut usize> {
        Some(&mut self.main_cursor)
    }

    fn new_moment(&mut self) {
        self.new_moment();
    }

    fn undo(&mut self) {
        self.undo()
    }

    fn redo(&mut self) {
        self.redo()
    }
}

unsafe impl<M> Send for FileWidget<M> where M: Ui {}
