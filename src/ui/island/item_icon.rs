use super::*;
use std::collections::VecDeque;
use std::path::Path;

// Only visible shortcuts load shell icons. Bound the cache and destroy the
// HICON returned by the shell when its entry leaves the cache.
struct Icon(HICON);
impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            DestroyIcon(self.0);
        }
    }
}
thread_local! {static ICONS:RefCell<VecDeque<(PathBuf,Option<Icon>)>>=const{RefCell::new(VecDeque::new())};}
pub(super) fn draw(dc: HDC, path: &Path, x: i32, y: i32, side: i32) -> bool {
    ICONS.with_borrow_mut(|cache| {
        let previous = cache
            .iter()
            .position(|(p, _)| p == path)
            .and_then(|i| cache.remove(i));
        let entry = previous.unwrap_or_else(|| {
            let mut info = SHFILEINFOW::default();
            let name: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
            let result = unsafe {
                SHGetFileInfoW(
                    name.as_ptr(),
                    0,
                    &mut info,
                    size_of::<SHFILEINFOW>() as u32,
                    SHGFI_ICON | SHGFI_SMALLICON,
                )
            };
            (
                path.to_owned(),
                (result != 0 && !info.hIcon.is_null()).then(|| Icon(info.hIcon)),
            )
        });
        let drawn = entry.1.as_ref().is_some_and(|icon| unsafe {
            DrawIconEx(dc, x, y, icon.0, side, side, 0, null_mut(), DI_NORMAL) != 0
        });
        cache.push_front(entry);
        cache.truncate(32);
        drawn
    })
}
