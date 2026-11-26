// Expanded item layout. See .agents/notes/module.md#当前工程.
use super::*;

pub(super) const COLUMNS: usize = 4;
pub(super) const PAGE_SIZE: usize = COLUMNS * 3;
pub(super) const BAR_CELL: i32 = 64;
pub(super) const BAR_DEPTH: i32 = 62;
const WIDTH: i32 = 80;
const HEIGHT: i32 = 58;
const PAD: i32 = 6;

pub(super) fn width(count: usize) -> i32 {
    2 * PAD + WIDTH * count.clamp(2, COLUMNS) as i32
}

pub(super) fn height(count: usize, footer: bool) -> i32 {
    2 * PAD
        + HEIGHT * count.clamp(1, PAGE_SIZE).div_ceil(COLUMNS) as i32
        + if footer { 36 } else { 0 }
}

pub(super) fn cell(body: RECT, count: usize, index: usize) -> [i32; 4] {
    let cols = count.clamp(1, COLUMNS);
    let span = body.right - body.left;
    let step = ((span - 2 * PAD) / cols as i32).clamp(1, WIDTH);
    let left = body.left + (span - step * cols as i32) / 2;
    [
        left + (index % cols) as i32 * step + 1,
        body.top + PAD + (index / cols) as i32 * HEIGHT,
        (step - 2).max(1),
        HEIGHT - 2,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_fit_the_visible_rows_and_leave_room_for_paging() {
        for count in [1, 2, 4, 5, 12, 13, 41] {
            let footer = count > PAGE_SIZE;
            let body = area(40, 0, 40 + width(count), height(count, footer));
            for i in 0..count.min(PAGE_SIZE) {
                let [x, y, w, h] = cell(body, count, i);
                assert!(x >= body.left && x + w <= body.right);
                assert!(y >= body.top && y + h <= body.bottom - if footer { 36 } else { 0 });
                if i >= COLUMNS {
                    let [above_x, above_y, _, above_h] = cell(body, count, i - COLUMNS);
                    assert_eq!(above_x, x);
                    assert!(above_y + above_h < y);
                }
            }
        }
    }
}
