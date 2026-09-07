use super::*;
use crossterm::event::KeyCode;
use crossterm::event::KeyModifiers;
use pretty_assertions::assert_eq;

#[test]
fn picker_filters_without_stealing_printable_navigation_aliases() {
    let spec = PickerSpec {
        title: "Models".into(),
        items: vec![
            PickerItem {
                label: "juniper".into(),
                description: "fast model".into(),
                command: "/model juniper".into(),
            },
            PickerItem {
                label: "maple".into(),
                description: "larger model".into(),
                command: "/model maple".into(),
            },
        ],
    };
    let mut picker = Picker::new(spec, Arc::new(RuntimeKeymap::defaults())).unwrap();
    picker.key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
    let area = Rect::new(
        /*x*/ 0, /*y*/ 0, /*width*/ 48, /*height*/ 5,
    );
    let mut buffer = Buffer::empty(area);
    assert_eq!(picker.render(area, &mut buffer), Some((10, 0)));
    let visible = (0..area.height)
        .map(|y| {
            (0..area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect::<Vec<_>>()
        .join("\n");
    insta::assert_snapshot!(visible);
    let PickerAction::Select(command) =
        picker.key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE))
    else {
        panic!("expected selection");
    };
    assert_eq!(command, "/model juniper");
}
