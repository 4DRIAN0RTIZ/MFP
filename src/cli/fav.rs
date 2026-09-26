use crate::operations::favorites::Favorites;
use anyhow::Result;

/// Adds, removes and/or lists favorite episodes.
pub(super) fn manage_favorites(
    add: Option<String>,
    remove: Option<String>,
    list: bool,
) -> Result<()> {
    let mut favorites = Favorites::load()?;

    if let Some(title) = add {
        if favorites.add(title.clone()) {
            println!("* Added: {}", title);
        } else {
            println!("Already in favorites: {}", title);
        }
    }

    if let Some(title) = remove {
        if favorites.remove(&title) {
            println!("Removed: {}", title);
        } else {
            println!("Not in favorites: {}", title);
        }
    }

    if list {
        let fav_list = favorites.list();
        if fav_list.is_empty() {
            println!("No favorites saved");
        } else {
            println!("Favorites:");
            for title in fav_list {
                println!("  * {}", title);
            }
        }
    }

    Ok(())
}
