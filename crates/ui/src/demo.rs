//! Spike: a context menu, a popover and a tooltip from Zed's `ui`, to show it works outside Zed,
//! and our text input to try it by hand. Replaced by the design system (PLAN.md 1.4).

use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable};
use theme::ActiveTheme as _;
use zed_ui::{
    Button, ContextMenu, Label, Popover, PopoverMenu, Tooltip, prelude::*, right_click_menu, v_flex,
};

use crate::design::text_input::TextInput;

pub struct Demo {
    input: Entity<TextInput>,
}

impl Demo {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            input: cx.new(|cx| TextInput::new("Type here…", cx)),
        }
    }
}

impl Render for Demo {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        v_flex()
            .size_full()
            .p_4()
            .gap_4()
            .items_start()
            .bg(colors.background)
            .text_color(colors.text)
            .child(
                right_click_menu("context-menu")
                    .trigger(|_, _, _| Label::new("Right-click here for a context menu"))
                    .menu(|window, cx| {
                        ContextMenu::build(window, cx, |menu, _, _| {
                            menu.entry("Archive", None, |_, _| {})
                                .entry("Delete", None, |_, _| {})
                        })
                    }),
            )
            .child(
                PopoverMenu::new("popover")
                    .trigger(Button::new("popover-trigger", "Open a popover"))
                    .menu(|_, cx| Some(cx.new(|cx| PopoverContent(cx.focus_handle())))),
            )
            .child(
                Button::new("tooltip-trigger", "Hover for a tooltip")
                    .tooltip(Tooltip::text("A tooltip from Zed's ui")),
            )
            .child(div().w_80().child(self.input.clone()))
    }
}

/// `PopoverMenu` anchors the popover to its button; closing it on an outside click is up to the content.
struct PopoverContent(FocusHandle);

impl Focusable for PopoverContent {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.0.clone()
    }
}

impl EventEmitter<DismissEvent> for PopoverContent {}

impl Render for PopoverContent {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .on_mouse_down_out(cx.listener(|_, _, _, cx| cx.emit(DismissEvent)))
            .child(Popover::new().child(div().px_2().child(Label::new("A popover from Zed's ui"))))
    }
}
