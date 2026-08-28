//! Guard the legacy macOS icon's transparent margin and rounded silhouette.

use std::path::Path;

#[test]
fn app_icons_have_consistent_macos_shape_and_padding() {
    let icons = Path::new(env!("CARGO_MANIFEST_DIR")).join("icons");
    for (name, size) in [
        ("32x32.png", 32),
        ("128x128.png", 128),
        ("128x128@2x.png", 256),
    ] {
        let image = image::open(icons.join(name)).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (size, size), "{name}");

        // A fully opaque square or a frame scaled to the full canvas must fail.
        for coordinate in 0..size {
            for (x, y) in [
                (coordinate, 0),
                (coordinate, size - 1),
                (0, coordinate),
                (size - 1, coordinate),
            ] {
                assert_eq!(
                    image.get_pixel(x, y)[3],
                    0,
                    "{name}: outer edge must be transparent"
                );
            }
        }
        let opaque: Vec<_> = image
            .enumerate_pixels()
            .filter(|(_, _, p)| p[3] >= 250)
            .collect();
        let min_x = opaque.iter().map(|(x, _, _)| *x).min().unwrap();
        let max_x = opaque.iter().map(|(x, _, _)| *x).max().unwrap();
        let expected_inset = size as f64 * 100.0 / 1024.0;
        assert!(
            (min_x as f64 - expected_inset).abs() <= 1.0,
            "{name}: left inset"
        );
        assert!(
            ((size - 1 - max_x) as f64 - expected_inset).abs() <= 1.0,
            "{name}: right inset"
        );
        // Sample inside the square's corner but outside the curved edge, even at 32 px.
        let corner = (size as f64 * 0.12).floor() as u32;
        assert!(
            image.get_pixel(corner, corner)[3] < 32,
            "{name}: rounded corner"
        );
        let center = image.get_pixel(size / 2, size / 2);
        assert_eq!(center[3], 255, "{name}: opaque center");
        assert!(
            center[0] > 230 && center[1] > 230 && center[2] > 230,
            "{name}: preserve the white star"
        );
    }
}
