// Utility functions, typically inlined, for more readable code

use eframe::epaint::Color32;

use crate::{displayed_item::DisplayedItem, State};

impl State {
    #[inline]
    pub fn get_item_text_color(&self, item: &DisplayedItem) -> &Color32 {
        item.color()
            .and_then(|color| self.config.theme.colors.get(&color))
            .unwrap_or(&self.config.theme.foreground)
    }

    #[inline]
    pub fn show_statusbar(&self) -> bool {
        self.show_statusbar
            .unwrap_or_else(|| self.config.layout.show_statusbar())
    }

    #[inline]
    pub fn show_toolbar(&self) -> bool {
        self.show_toolbar
            .unwrap_or_else(|| self.config.layout.show_toolbar())
    }

    #[inline]
    pub fn show_overview(&self) -> bool {
        self.show_overview
            .unwrap_or_else(|| self.config.layout.show_overview())
    }

    #[inline]
    pub fn show_hierarchy(&self) -> bool {
        self.show_hierarchy
            .unwrap_or_else(|| self.config.layout.show_hierarchy())
    }

    #[inline]
    pub fn show_tooltip(&self) -> bool {
        self.show_tooltip
            .unwrap_or_else(|| self.config.layout.show_tooltip())
    }

    #[inline]
    pub fn get_show_ticks(&self) -> bool {
        self.show_ticks
            .unwrap_or_else(|| self.config.layout.show_ticks())
    }

    #[inline]
    pub fn show_menu(&self) -> bool {
        self.show_menu
            .unwrap_or_else(|| self.config.layout.show_menu())
    }

    #[inline]
    pub fn show_variable_indices(&self) -> bool {
        self.show_variable_indices
            .unwrap_or_else(|| self.config.layout.show_variable_indices())
    }
}
