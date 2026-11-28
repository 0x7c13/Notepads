use tiny_skia::*;

fn main() {
    let mut paint = Paint {
        shader: Shader::SolidColor(Color::from_rgba8(255, 100, 20, 255)),
        anti_alias: true,
        ..Default::default()
    };
    let stroke = Stroke::default();

    let mut pixmap = Pixmap::new(1000, 1000).unwrap();
    pixmap.fill(Color::BLACK);

    let mut pb = PathBuilder::new();
    for i in 0..10 {
        pb.move_to(50.0, 45.0 + i as f32 * 20.0);
        pb.line_to(450.0, 45.0 + i as f32 * 21.0);
    }
    let path = pb.finish().unwrap();

    let colors = [
        ColorSpace::Linear,
        ColorSpace::Gamma2,
        ColorSpace::SimpleSRGB,
        ColorSpace::FullSRGBGamma,
    ];

    for (i, color) in colors.iter().enumerate() {
        paint.colorspace = *color;

        let mut xf = Transform::identity();
        xf = xf.pre_translate(0.0, 240.0 * i as f32);

        pixmap.stroke_path(&path, &paint, &stroke, xf, None);

        // Move down 0.5 pixel so lines start in the middle of the pixel, not the edge
        xf = xf.pre_translate(500.0, 0.5);

        pixmap.stroke_path(&path, &paint, &stroke, xf, None);
    }

    pixmap.save_png("image.png").unwrap();
}
