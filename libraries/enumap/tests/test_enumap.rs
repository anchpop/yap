use enumap::EnuMap;

#[derive(EnuMap, Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    Red,
    Green,
    Blue,
}

#[test]
fn test_basic_enumap() {
    let map = ColorMap {
        red: 255,
        green: 128,
        blue: 64,
    };

    assert_eq!(map.get(&Color::Red), &255);
    assert_eq!(map.get(&Color::Green), &128);
    assert_eq!(map.get(&Color::Blue), &64);
}

#[test]
fn test_enumap_mut() {
    let mut map = ColorMap {
        red: 0,
        green: 0,
        blue: 0,
    };

    *map.get_mut(&Color::Red) = 100;
    *map.get_mut(&Color::Green) = 200;

    assert_eq!(map.get(&Color::Red), &100);
    assert_eq!(map.get(&Color::Green), &200);
    assert_eq!(map.get(&Color::Blue), &0);
}
