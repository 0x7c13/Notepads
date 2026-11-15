use tiny_skia::*;

#[test]
fn gamma() {
    let mut paint = Paint::default();
    let stroke = Stroke::default();
    let wide = Stroke {
        width: 3.0,
        ..Default::default()
    };

    let solid = Shader::SolidColor(Color::from_rgba8(255, 100, 20, 255));
    let grad2 = LinearGradient::new(
        Point { x: 50.0, y: 2.0 },
        Point { x: 450.0, y: 2.0 },
        vec![
            GradientStop::new(0.0, Color::from_rgba8(255, 0, 0, 255)),
            GradientStop::new(1.0, Color::from_rgba8(0, 255, 0, 255)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();

    let grad3 = LinearGradient::new(
        Point { x: 50.0, y: 2.0 },
        Point { x: 450.0, y: 2.0 },
        vec![
            GradientStop::new(0.0, Color::from_rgba8(255, 0, 0, 255)),
            GradientStop::new(0.5, Color::from_rgba8(0, 0, 255, 255)),
            GradientStop::new(1.0, Color::from_rgba8(128, 128, 128, 128)),
        ],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .unwrap();

    let mut pixmap = Pixmap::new(500, 60).unwrap();
    pixmap.fill(Color::BLACK);

    let mut pb = PathBuilder::new();
    pb.move_to(20.0, 2.0);
    pb.line_to(480.0, 3.0);
    let path = pb.finish().unwrap();

    let colors = [
        ColorSpace::Linear,
        ColorSpace::Gamma2,
        ColorSpace::SimpleSRGB,
        ColorSpace::FullSRGBGamma,
    ];

    for (i, color) in colors.iter().enumerate() {
        let xf = Transform::from_translate(0.0, 4.0 * i as f32);

        paint.colorspace = *color;
        paint.shader = solid.clone();
        pixmap.stroke_path(&path, &paint, &stroke, xf, None);

        let xf = Transform::from_translate(0.0, 20.0 + 10.0 * i as f32);
        paint.shader = grad2.clone();
        pixmap.stroke_path(&path, &paint, &wide, xf, None);

        let xf = Transform::from_translate(0.0, 22.5 + 10.0 * i as f32);
        paint.shader = grad3.clone();
        pixmap.stroke_path(&path, &paint, &wide, xf, None);
    }

    // pixmap.save_png("tests/images/gamma.png").unwrap();
    let expected = Pixmap::load_png("tests/images/gamma.png").unwrap();
    assert_eq!(pixmap, expected);
}
