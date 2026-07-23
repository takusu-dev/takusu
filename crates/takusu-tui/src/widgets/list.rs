#[derive(Default)]
pub struct StatefulList {
    pub index: usize,
    pub len: usize,
    pub scroll: usize,
}

impl StatefulList {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_len(&mut self, len: usize) {
        self.len = len;
        if self.index >= len && len > 0 {
            self.index = len - 1;
        }
        if len == 0 {
            self.index = 0;
            self.scroll = 0;
        }
    }

    pub fn selected(&self) -> Option<usize> {
        if self.len == 0 {
            None
        } else {
            Some(self.index)
        }
    }

    pub fn next(&mut self) {
        if self.len == 0 {
            return;
        }
        self.index = (self.index + 1).min(self.len - 1);
    }

    pub fn prev(&mut self) {
        self.index = self.index.saturating_sub(1);
    }

    pub fn ensure_visible(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        if self.index < self.scroll {
            self.scroll = self.index;
        } else if self.index >= self.scroll + viewport_height {
            self.scroll = self.index - viewport_height + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_list_has_no_selection() {
        let list = StatefulList::new();
        assert_eq!(list.selected(), None);
        assert_eq!(list.len, 0);
    }

    #[test]
    fn set_len_clamps_index() {
        let mut list = StatefulList::new();
        list.index = 5;
        list.set_len(3);
        assert_eq!(list.index, 2);
        assert_eq!(list.selected(), Some(2));
    }

    #[test]
    fn next_and_prev_stay_in_bounds() {
        let mut list = StatefulList::new();
        list.set_len(3);
        assert_eq!(list.selected(), Some(0));
        list.next();
        assert_eq!(list.selected(), Some(1));
        list.prev();
        assert_eq!(list.selected(), Some(0));
        list.prev();
        assert_eq!(list.selected(), Some(0));
        list.index = 2;
        list.next();
        assert_eq!(list.selected(), Some(2));
    }

    #[test]
    fn ensure_visible_scrolls_down() {
        let mut list = StatefulList::new();
        list.set_len(10);
        list.index = 7;
        list.ensure_visible(5);
        assert_eq!(list.scroll, 3);
    }

    #[test]
    fn ensure_visible_scrolls_up() {
        let mut list = StatefulList::new();
        list.set_len(10);
        list.index = 7;
        list.scroll = 8;
        list.ensure_visible(5);
        assert_eq!(list.scroll, 7);
    }
}
