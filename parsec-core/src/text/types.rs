use std::rc::Rc;

use crossterm::event::MouseEventKind;

use super::tags::{RawTag, ToggleId};
use crate::{forms::FormId, position::Point, PALETTE};

/// A part of the [`Text`], can be a [`char`] or a [`Tag`].
#[derive(Clone, Copy)]
pub enum Part {
    Char(char),
    PushForm(FormId),
    PopForm(FormId),
    MainCursor,
    ExtraCursor,
    AlignLeft,
    AlignCenter,
    AlignRight,
    ToggleStart(ToggleId),
    ToggleEnd(ToggleId),
    Termination,
}

impl Part {
    // TODO: Add a default alignment.
    pub(super) fn from_raw(value: RawTag) -> Self {
        match value {
            RawTag::PushForm((_, id)) => Part::PushForm(id),
            RawTag::PopForm((_, id)) => Part::PopForm(id),
            RawTag::MainCursor(_) => Part::MainCursor,
            RawTag::ExtraCursor(_) => Part::ExtraCursor,
            RawTag::StartAlignLeft(_) => Part::AlignLeft,
            RawTag::EndAlignLeft(_) => Part::AlignLeft,
            RawTag::StartAlignCenter(_) => Part::AlignCenter,
            RawTag::EndAlignCenter(_) => Part::AlignLeft,
            RawTag::StartAlignRight(_) => Part::AlignRight,
            RawTag::EndAlignRight(_) => Part::AlignLeft,
            RawTag::ToggleStart((_, id)) => Part::ToggleStart(id),
            RawTag::ToggleEnd((_, id)) => Part::ToggleEnd(id),
            RawTag::Concealed(_) => Part::Termination,
            RawTag::ConcealStart(_) | RawTag::ConcealEnd(_) | RawTag::GhostText(..) => {
                unreachable!("These tags are automatically processed elsewhere.")
            }
        }
    }
}

impl std::fmt::Debug for Part {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Part::Char(char) => f.debug_tuple("Char").field(char).finish(),
            Part::PushForm(id) => f
                .debug_tuple("PushForm")
                .field(&PALETTE.name_from_id(*id))
                .finish(),
            Part::PopForm(id) => f
                .debug_tuple("PopForm")
                .field(&PALETTE.name_from_id(*id))
                .finish(),
            Part::MainCursor => f.debug_tuple("MainCursor").finish(),
            Part::ExtraCursor => f.debug_tuple("ExtraCursor").finish(),
            Part::AlignLeft => f.debug_tuple("AlignLeft").finish(),
            Part::AlignCenter => f.debug_tuple("AlignCenter ").finish(),
            Part::AlignRight => f.debug_tuple("AlignRight ").finish(),
            Part::ToggleStart(_) => f.debug_tuple("ToggleStart").finish(),
            Part::ToggleEnd(_) => f.debug_tuple("ToggleEnd").finish(),
            Part::Termination => f.debug_tuple("Termination ").finish(),
        }
    }
}

impl Part {
    /// Returns `true` if the text bit is [`Char`].
    ///
    /// [`Char`]: TextBit::Char
    #[must_use]
    pub fn is_char(&self) -> bool {
        matches!(self, Part::Char(_))
    }

    pub fn as_char(&self) -> Option<char> {
        if let Self::Char(v) = self {
            Some(*v)
        } else {
            None
        }
    }

    pub fn is_tag(&self) -> bool {
        !self.is_char()
    }
}

pub type Toggle = Rc<dyn Fn(Point, MouseEventKind) + Send + Sync>;
