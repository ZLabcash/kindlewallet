//! The screens' flow, through taps where a finger would land (600×800, as on the Kindle).

use kindlewallet::app::{Action, App, Effect};
use kindlewallet::coins::Coin;

const PIN: [u8; 6] = [4, 8, 2, 9, 1, 3];

fn unlock(dir: &std::path::Path) -> App {
    let mut app = App::new(dir.to_path_buf());
    for d in PIN {
        app.act(Action::Digit(d));
    }
    app.act(Action::PinOk);
    app
}

/// Taps the middle of the button whose label is `label` on the current page.
fn tap_label(app: &mut App, label: &str) {
    app.draw(600, 800);
    let (x, y) = app.button_at(label).unwrap_or_else(|| panic!("no button {label:?} on {}", app.debug_screen()));
    app.tap(x, y);
    app.draw(600, 800);
}

#[test]
fn unlocked_the_link_is_up_locked_it_is_down() {
    let dir = std::env::temp_dir().join("kw_ui_link");
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = App::new(dir.clone());
    app.act(Action::Create);
    app.act(Action::WroteThem);
    for _ in 0..2 {
        for d in PIN {
            app.act(Action::Digit(d));
        }
        app.act(Action::PinOk);
    }
    assert!(app.take_effects().contains(&Effect::LinkOn), "a new wallet opens with the link up");
    // Browsing does not touch it; "Phone link" is the QR code's screen.
    app.act(Action::Open(Coin::Zcash));
    tap_label(&mut app, "Phone link");
    assert_eq!(app.debug_screen(), "Link");
    app.act(Action::Home);
    assert!(app.take_effects().is_empty());
    app.act(Action::Lock);
    assert!(app.take_effects().contains(&Effect::LinkOff));
    // Opened again: up again.
    let mut again = unlock(&dir);
    assert_eq!(again.debug_screen(), "Home");
    assert!(again.take_effects().contains(&Effect::LinkOn));
}
