//! Pure logic behind the episode list: filtering, selection and scrolling.

/// A selection movement requested by the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListMove {
    /// One row up.
    Up,
    /// One row down.
    Down,
    /// One page up.
    PageUp,
    /// One page down.
    PageDown,
    /// First row.
    Home,
    /// Last row.
    End,
}

/// Indices of the titles that contain `query`, case-insensitively.
///
/// An empty (or whitespace-only) query matches everything. Feed order is kept.
pub fn filter_indices(titles: &[String], query: &str) -> Vec<usize> {
    let needle = query.trim().to_lowercase();
    titles
        .iter()
        .enumerate()
        .filter(|(_, title)| needle.is_empty() || title.to_lowercase().contains(&needle))
        .map(|(i, _)| i)
        .collect()
}

/// New selection after `movement` in a list of `len` rows.
///
/// The result always stays inside `0..len` (0 for an empty list) and clamps at
/// both ends instead of wrapping. `page` is the number of rows per page.
pub fn move_selection(selected: usize, len: usize, movement: ListMove, page: usize) -> usize {
    if len == 0 {
        return 0;
    }
    let last = len - 1;
    let page = page.max(1);
    let selected = selected.min(last);
    match movement {
        ListMove::Up => selected.saturating_sub(1),
        ListMove::Down => (selected + 1).min(last),
        ListMove::PageUp => selected.saturating_sub(page),
        ListMove::PageDown => (selected + page).min(last),
        ListMove::Home => 0,
        ListMove::End => last,
    }
}

/// First visible row so that `selected` is on screen, moving `offset` as little
/// as possible. `height` is the number of visible rows and `len` the list size.
pub fn scroll_offset(offset: usize, selected: usize, height: usize, len: usize) -> usize {
    if height == 0 || len <= height {
        return 0;
    }
    let selected = selected.min(len - 1);
    let mut offset = offset;
    if selected < offset {
        offset = selected;
    } else if selected >= offset + height {
        offset = selected + 1 - height;
    }
    offset.min(len - height)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles() -> Vec<String> {
        [
            "Episode 78: Ben Frost",
            "Episode 77: Anonymous",
            "Episode 75: Datassette",
            "Episode 74: NCW",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn filter_is_case_insensitive_substring_over_the_whole_title() {
        let t = titles();
        assert_eq!(filter_indices(&t, "datas"), vec![2]);
        assert_eq!(filter_indices(&t, "DATAS"), vec![2]);
        assert_eq!(filter_indices(&t, "75"), vec![2]);
        assert_eq!(filter_indices(&t, "episode 7"), vec![0, 1, 2, 3]);
        assert_eq!(filter_indices(&t, "zzz"), Vec::<usize>::new());
    }

    #[test]
    fn empty_query_matches_everything_in_order() {
        let t = titles();
        assert_eq!(filter_indices(&t, ""), vec![0, 1, 2, 3]);
        assert_eq!(filter_indices(&t, "   "), vec![0, 1, 2, 3]);
        assert!(filter_indices(&[], "x").is_empty());
    }

    #[test]
    fn selection_clamps_at_both_ends() {
        assert_eq!(move_selection(0, 5, ListMove::Up, 3), 0);
        assert_eq!(move_selection(4, 5, ListMove::Down, 3), 4);
        assert_eq!(move_selection(1, 5, ListMove::Down, 3), 2);
        assert_eq!(move_selection(2, 5, ListMove::Up, 3), 1);
        assert_eq!(move_selection(1, 5, ListMove::PageUp, 3), 0);
        assert_eq!(move_selection(3, 5, ListMove::PageDown, 3), 4);
        assert_eq!(move_selection(2, 5, ListMove::Home, 3), 0);
        assert_eq!(move_selection(2, 5, ListMove::End, 3), 4);
    }

    #[test]
    fn selection_handles_empty_lists_and_stale_indices() {
        for m in [
            ListMove::Up,
            ListMove::Down,
            ListMove::PageUp,
            ListMove::PageDown,
            ListMove::Home,
            ListMove::End,
        ] {
            assert_eq!(move_selection(7, 0, m, 5), 0);
        }
        // A selection beyond the end (list shrank) is pulled back first.
        assert_eq!(move_selection(9, 3, ListMove::Down, 5), 2);
        // A zero page still moves.
        assert_eq!(move_selection(0, 5, ListMove::PageDown, 0), 1);
    }

    #[test]
    fn scroll_follows_the_selection_minimally() {
        // Fits entirely: never scrolls.
        assert_eq!(scroll_offset(3, 2, 10, 5), 0);
        // Selection below the window scrolls just enough.
        assert_eq!(scroll_offset(0, 5, 5, 20), 1);
        // Selection above the window scrolls up to it.
        assert_eq!(scroll_offset(8, 6, 5, 20), 6);
        // Inside the window: unchanged.
        assert_eq!(scroll_offset(4, 6, 5, 20), 4);
        // Last row: offset is capped so the window stays full.
        assert_eq!(scroll_offset(0, 19, 5, 20), 15);
        // A stale offset past the end is clamped.
        assert_eq!(scroll_offset(50, 19, 5, 20), 15);
        assert_eq!(scroll_offset(0, 0, 0, 20), 0);
    }
}
