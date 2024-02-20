use ratatui::{prelude::*, widgets::Widget};

use crate::{items::TargetData, theme::CURRENT_THEME, Res};

use super::Item;
use std::{borrow::Cow, collections::HashSet};

pub(crate) mod diff;
pub(crate) mod log;
pub(crate) mod show;
pub(crate) mod show_refs;
pub(crate) mod status;

pub(crate) struct Screen {
    pub(crate) cursor: usize,
    pub(crate) scroll: u16,
    pub(crate) size: Rect,
    refresh_items: Box<dyn Fn() -> Res<Vec<Item>>>,
    items: Vec<Item>,
    collapsed: HashSet<Cow<'static, str>>,
}

impl<'a> Screen {
    pub(crate) fn new(size: Rect, refresh_items: Box<dyn Fn() -> Res<Vec<Item>>>) -> Res<Self> {
        let items = refresh_items()?;

        let mut screen = Self {
            cursor: 0,
            scroll: 0,
            size,
            refresh_items,
            items,
            collapsed: HashSet::new(),
        };

        screen.cursor = screen
            .find_first_hunk()
            .or_else(|| screen.find_first_selectable())
            .unwrap_or(0);

        Ok(screen)
    }

    fn find_first_hunk(&mut self) -> Option<usize> {
        self.collapsed_items_iter()
            .find(|(_i, item)| {
                !item.unselectable && matches!(item.target_data, Some(TargetData::Hunk(_)))
            })
            .map(|(i, _item)| i)
    }

    fn find_first_selectable(&mut self) -> Option<usize> {
        self.collapsed_items_iter()
            .find(|(_i, item)| !item.unselectable)
            .map(|(i, _item)| i)
    }

    pub(crate) fn select_next(&mut self) {
        self.cursor = self.find_next();
        self.scroll_fit_end();
        self.scroll_fit_start();
    }

    fn scroll_fit_start(&mut self) {
        if self.items.is_empty() {
            return;
        }

        let start_line = self
            .collapsed_lines_items_iter()
            .find(|(_line, i, item, _lc)| self.selected_or_direct_ancestor(item, i))
            .map(|(line, _i, _item, _lc)| line)
            .unwrap() as u16;

        if start_line < self.scroll {
            self.scroll = start_line;
        }
    }

    fn selected_or_direct_ancestor(&self, item: &Item, i: &usize) -> bool {
        let levels_above = self.get_selected_item().depth.saturating_sub(item.depth);
        i == &(self.cursor - levels_above)
    }

    fn scroll_fit_end(&mut self) {
        if self.items.is_empty() {
            return;
        }

        let depth = self.items[self.cursor].depth;
        let last = 1 + self
            .collapsed_lines_items_iter()
            .skip_while(|(_line, i, _item, _lc)| i < &self.cursor)
            .take_while(|(_line, i, item, _lc)| i == &self.cursor || depth < item.depth)
            .map(|(line, _i, _item, lc)| line + lc)
            .last()
            .unwrap();

        let end_line = self.size.height.saturating_sub(1);
        if last as u16 > end_line + self.scroll {
            self.scroll = last as u16 - end_line;
        }
    }

    pub(crate) fn find_next(&mut self) -> usize {
        self.collapsed_items_iter()
            .find(|(i, item)| i > &self.cursor && !item.unselectable)
            .map(|(i, _item)| i)
            .unwrap_or(self.cursor)
    }

    pub(crate) fn select_previous(&mut self) {
        self.cursor = self
            .collapsed_items_iter()
            .filter(|(i, item)| i < &self.cursor && !item.unselectable)
            .last()
            .map(|(i, _item)| i)
            .unwrap_or(self.cursor);

        self.scroll_fit_start();
    }

    pub(crate) fn scroll_half_page_up(&mut self) {
        let half_screen = self.size.height / 2;
        self.scroll = self.scroll.saturating_sub(half_screen);
    }

    pub(crate) fn scroll_half_page_down(&mut self) {
        let half_screen = self.size.height / 2;
        self.scroll = (self.scroll + half_screen).min(
            self.collapsed_lines_items_iter()
                .map(|(line, _i, _item, lc)| (line + lc).saturating_sub(half_screen as usize))
                .last()
                .unwrap_or(0) as u16,
        );
    }

    fn collapsed_lines_items_iter(&'a self) -> impl Iterator<Item = (usize, usize, &Item, usize)> {
        self.collapsed_items_iter().scan(0, |lines, (i, item)| {
            let line = *lines;
            let lc = item.display.lines.len();
            *lines += lc;

            Some((line, i, item, lc))
        })
    }

    pub(crate) fn toggle_section(&mut self) {
        let selected = &self.items[self.cursor];

        if selected.section {
            if self.collapsed.contains(&selected.id) {
                self.collapsed.remove(&selected.id);
            } else {
                self.collapsed.insert(selected.id.clone());
            }
        }
    }

    pub(crate) fn clamp_cursor(&mut self) {
        self.cursor = self.cursor.clamp(0, self.items.len().saturating_sub(1))
    }

    pub(crate) fn update(&mut self) -> Res<()> {
        self.items = (self.refresh_items)()?;
        Ok(())
    }

    pub(crate) fn collapsed_items_iter(&'a self) -> impl Iterator<Item = (usize, &Item)> {
        self.items
            .iter()
            .enumerate()
            .scan(None, |collapse_depth, (i, next)| {
                if collapse_depth.is_some_and(|depth| depth < next.depth) {
                    return Some(None);
                }

                *collapse_depth = if next.section && self.is_collapsed(next) {
                    Some(next.depth)
                } else {
                    None
                };

                Some(Some((i, next)))
            })
            .flatten()
    }

    fn is_collapsed(&self, item: &Item) -> bool {
        self.collapsed.contains(&item.id)
    }

    pub(crate) fn get_selected_item(&self) -> &Item {
        &self.items[self.cursor]
    }

    fn display_lines(&self) -> impl Iterator<Item = (usize, &Item, &Line<'_>)> {
        self.collapsed_items_iter()
            .flat_map(|(i, item)| item.display.lines.iter().map(move |line| (i, item, line)))
    }
}

impl Widget for &Screen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let mut highlight_depth = None;
        for (line_i, (item_i, item, line)) in self
            .display_lines()
            .skip(self.scroll as usize)
            .take(area.height as usize)
            .enumerate()
        {
            if self.cursor == item_i {
                highlight_depth = Some(item.depth);
            } else if highlight_depth.is_some_and(|s| s >= item.depth) {
                highlight_depth = None;
            };

            if highlight_depth.is_some() {
                let area = Rect {
                    x: 0,
                    y: line_i as u16,
                    width: buf.area.width,
                    height: 1,
                };

                buf.set_style(
                    area,
                    Style::new().bg(if self.cursor == item_i {
                        CURRENT_THEME.highlight
                    } else {
                        CURRENT_THEME.dim_highlight
                    }),
                );
            }

            let mut x = 1;
            let mut overflow = false;
            for span in line.spans.iter() {
                buf.set_span(x, line_i as u16, span, span.width() as u16);
                overflow = x + span.width() as u16 >= area.width;
                if overflow {
                    x = area.width - 1;
                    break;
                }
                x += span.width() as u16;
            }

            if self.is_collapsed(item) && line.width() > 0 || overflow {
                buf.get_mut(x, line_i as u16).set_char('…');
            }
            if self.cursor == item_i {
                buf.get_mut(0, line_i as u16).set_char('🢒');
            }
        }
    }
}