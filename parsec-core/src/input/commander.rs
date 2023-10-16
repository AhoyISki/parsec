use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::{key, Cursors, InputMethod, MultiCursorEditor};
use crate::{data::RwData, ui::Area, widgets::CommandLine, commands::{self}};

#[derive(Clone)]
pub struct Commander {
    cursors: Cursors,
}

impl Commander {
    pub fn new() -> Self {
        Self {
            cursors: Cursors::new(),
        }
    }
}

impl Default for Commander {
    fn default() -> Self {
        Self::new()
    }
}

impl InputMethod for Commander {
    type Widget = CommandLine;

    fn send_key(&mut self, key: KeyEvent, widget: &RwData<Self::Widget>, area: &impl Area) {
        let mut editor = MultiCursorEditor::new(widget, &mut self.cursors, area);

        match key {
            key!(KeyCode::Backspace) => {
                editor.move_main(|mover| {
                    mover.set_anchor();
                    mover.move_hor(-1);
                });
                editor.edit_on_main(|editor| {
                    editor.replace("");
                });
                editor.move_main(|mover| {
                    mover.unset_anchor();
                });
            }
            key!(KeyCode::Delete) => {
                editor.move_main(|mover| {
                    mover.set_anchor();
                    mover.move_hor(1);
                });
                editor.edit_on_main(|editor| {
                    editor.replace("");
                });
                editor.move_main(|mover| {
                    mover.unset_anchor();
                });
            }
            key!(KeyCode::Char(ch)) => {
                editor.edit_on_main(|editor| editor.insert(ch));
                editor.move_main(|mover| mover.move_hor(1));
            }
            key!(KeyCode::Char(ch), KeyModifiers::SHIFT) => {
                editor.edit_on_main(|editor| editor.insert(ch));
                editor.move_main(|mover| mover.move_hor(1));
            }

            key!(KeyCode::Left) => {
                editor.move_main(|mover| {
                    mover.unset_anchor();
                    mover.move_hor(-1)
                });
            }
            key!(KeyCode::Right) => {
                editor.move_main(|mover| {
                    mover.unset_anchor();
                    mover.move_hor(1)
                });
            }

            key!(KeyCode::Esc) => {
                editor.move_main(|mover| {
                    mover.move_hor(isize::MIN);
                    mover.set_anchor();
                    mover.move_hor(isize::MAX);
                });

                editor.edit_on_main(|editor| editor.replace(""));

				self.cursors = Cursors::default();
                commands::return_to_file().unwrap();
            }
            key!(KeyCode::Enter) => {
				self.cursors = Cursors::default();
                commands::return_to_file().unwrap();
            }
            _ => {}
        }
    }

    fn cursors(&self) -> Option<&Cursors> {
        Some(&self.cursors)
    }
}
