use futures::executor::block_on;
use merman_core::{Engine, ParseOptions};
use merman_render::LayoutOptions;
use merman_render::environment::RenderEnvironment;
use merman_render::family;
use merman_render::svg::{SvgDebugOptions, SvgRenderOptions};
use std::io::Read;

fn main() {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("read stdin");

    let engine = Engine::new();
    let parsed = block_on(engine.parse_diagram_for_render_model(&input, ParseOptions::default()))
        .expect("parse ok")
        .expect("diagram detected");

    let session = RenderEnvironment::deterministic()
        .begin_session()
        .expect("begin render session");
    let artifact = family::prepare(parsed, &LayoutOptions::default(), session).expect("layout ok");
    let rendered = artifact
        .render_svg(&SvgRenderOptions::default(), &SvgDebugOptions::default())
        .expect("render svg");
    print!("{}", rendered.svg());
}
