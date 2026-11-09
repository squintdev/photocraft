//! Evidence for #11826: which editing paths keep an Indexed document inside its colour table.
//!
//! One test per tool in the reporter's matrix. Each builds the same gradient document converted
//! to Indexed Color with the `exact` palette (the table holds the image's own colours, no
//! dither), runs the tool with an out-of-table foreground or geometry, and asserts every pixel
//! of every layer is still a table colour.

use photocraft_doc::Document;
use photocraft_engine::Session;
use serde_json::json;

fn doc(s: &Session) -> &Document {
    &s.active().unwrap().doc
}

/// A 40x20 RGB document converted to Indexed Color with the exact palette (no dither): the
/// table holds the image's own colours. Two contents: a strict two-colour split (the pixel-art
/// case) and a per-column gradient (many near colours).
type Paint = fn(i32, i32) -> [f32; 4];

fn indexed(paint: Paint) -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width": 40, "height": 20, "depth": 8, "mode": "rgb"})).unwrap();
    s.edit("matrix setup", |doc, active| {
        let b = doc.bounds();
        let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
        let fmt = surf.format();
        let mut data = Vec::with_capacity((b.width() * b.height()) as usize * fmt.channels());
        for y in b.y0..b.y1 {
            for x in b.x0..b.x1 {
                data.extend(photocraft_raster::from_rgba(&fmt, paint(x, y)));
            }
        }
        surf.write_region(b, &data);
        Ok(())
    })
    .unwrap();
    s.execute("tools.setColors", json!({"foreground": "#ff0000"})).unwrap();
    s.execute("image.mode.indexedColor", json!({"palette": "exact", "forced": "none", "dither": "none"})).unwrap();
    s
}

fn halves(x: i32, _: i32) -> [f32; 4] {
    if x < 20 { [0.0, 0.0, 0.0, 1.0] } else { [1.0, 1.0, 1.0, 1.0] }
}

fn gradient(x: i32, _: i32) -> [f32; 4] {
    [x as f32 / 40.0, 0.5, 0.5, 1.0]
}

/// Colours present on any layer that are not in the document's colour table.
fn off_table(s: &Session) -> Vec<[u8; 3]> {
    let d = doc(s);
    let t = d.color_table.as_ref().unwrap();
    let mut off = Vec::new();
    for layer in &d.layers {
        let Some(surf) = layer.surface() else { continue };
        for y in 0..d.size.height as i32 {
            for x in 0..d.size.width as i32 {
                let c = surf.rgba(x, y);
                if c[3] > 0.0 {
                    let u8 = [c[0], c[1], c[2]].map(|v| (v * 255.0).round() as u8);
                    if !t.colors.contains(&u8) && !off.contains(&u8) {
                        off.push(u8);
                    }
                }
            }
        }
    }
    off
}

type ToolRun = (&'static str, fn(&mut Session));

/// The tools and operations the #11826 reporter listed, each driven with an out-of-table
/// foreground and soft edges (the blending most likely to leave the table).
fn runs() -> Vec<ToolRun> {
    vec![
        ("paint.gradient", |s| {
            s.execute("paint.gradient", json!({"from": [5, 10], "to": [35, 10], "colors": ["#ff0000", "#00ff00"]})).unwrap();
        }),
        ("paint.bucket", |s| {
            s.execute("paint.bucket", json!({"x": 30, "y": 5, "color": "#ff0000"})).unwrap();
        }),
        ("paint.colorReplacement", |s| {
            s.execute("paint.colorReplacement", json!({"points": [[20, 10]], "size": 20, "mode": "color", "tolerance": 30})).unwrap();
        }),
        ("paint.mixerBrush", |s| {
            s.execute("paint.mixerBrush", json!({"points": [[20, 10], [30, 10]], "size": 20, "wet": 0, "mix": 0})).unwrap();
        }),
        ("paint.blur", |s| {
            s.execute("paint.blur", json!({"points": [[20, 10], [30, 10]], "size": 20, "hardness": 0, "strength": 100, "opacity": 50})).unwrap();
        }),
        ("paint.sharpen", |s| {
            s.execute("paint.sharpen", json!({"points": [[20, 10], [30, 10]], "size": 20, "hardness": 0, "strength": 100, "opacity": 50})).unwrap();
        }),
        ("paint.smudge", |s| {
            s.execute("paint.smudge", json!({"points": [[20, 10], [30, 10]], "size": 20, "hardness": 0, "strength": 100, "opacity": 50})).unwrap();
        }),
        ("paint.dodge", |s| {
            s.execute("paint.dodge", json!({"points": [[20, 10], [30, 10]], "size": 20, "hardness": 0, "opacity": 50})).unwrap();
        }),
        ("paint.burn", |s| {
            s.execute("paint.burn", json!({"points": [[20, 10], [30, 10]], "size": 20, "hardness": 0, "opacity": 50})).unwrap();
        }),
        ("paint.sponge", |s| {
            s.execute("paint.sponge", json!({"points": [[20, 10], [30, 10]], "size": 20, "hardness": 0, "mode": "saturate"})).unwrap();
        }),
        ("edit.paste", |s| {
            let mut src = Session::new();
            src.execute("file.new", json!({"width": 10, "height": 10, "background": "#ff0000"})).unwrap();
            src.execute("select.all", json!({})).unwrap();
            src.execute("edit.copy", json!({})).unwrap();
            let moved = src.clipboard.take().expect("the copy filled the clipboard");
            s.clipboard = Some(moved);
            s.execute("edit.paste", json!({})).unwrap();
        }),
        ("image.rotation.arbitrary bilinear", |s| {
            s.execute("image.rotation.arbitrary", json!({"angle": 30, "interpolation": "bilinear"})).unwrap();
        }),
    ]
}

fn run_tool(s: &mut Session, tool: &str) {
    for (name, run) in runs() {
        if name == tool {
            run(s);
            return;
        }
    }
    panic!("unknown tool {tool}");
}

fn assert_tool(palette_name: &str, paint: Paint, tool: &str) {
    let mut s = indexed(paint);
    run_tool(&mut s, tool);
    let off = off_table(&s);
    assert!(off.is_empty(), "{palette_name} / {tool}: off-table colours {off:?}");
}

/// paint.gradient: both palettes, out-of-table foreground, soft edges.
#[test]
fn paint_gradient_stays_in_the_table() {
    assert_tool("strict black and white", halves, "paint.gradient");
    assert_tool("per-column gradient", gradient, "paint.gradient");
}
/// paint.bucket: both palettes, out-of-table foreground, soft edges.
#[test]
fn paint_bucket_stays_in_the_table() {
    assert_tool("strict black and white", halves, "paint.bucket");
    assert_tool("per-column gradient", gradient, "paint.bucket");
}
/// paint.colorReplacement: both palettes, out-of-table foreground, soft edges.
#[test]
fn paint_colorReplacement_stays_in_the_table() {
    assert_tool("strict black and white", halves, "paint.colorReplacement");
    assert_tool("per-column gradient", gradient, "paint.colorReplacement");
}
/// paint.mixerBrush: both palettes, out-of-table foreground, soft edges.
#[test]
fn paint_mixerBrush_stays_in_the_table() {
    assert_tool("strict black and white", halves, "paint.mixerBrush");
    assert_tool("per-column gradient", gradient, "paint.mixerBrush");
}
/// paint.blur: both palettes, out-of-table foreground, soft edges.
#[test]
fn paint_blur_stays_in_the_table() {
    assert_tool("strict black and white", halves, "paint.blur");
    assert_tool("per-column gradient", gradient, "paint.blur");
}
/// paint.sharpen: both palettes, out-of-table foreground, soft edges.
#[test]
fn paint_sharpen_stays_in_the_table() {
    assert_tool("strict black and white", halves, "paint.sharpen");
    assert_tool("per-column gradient", gradient, "paint.sharpen");
}
/// paint.smudge: both palettes, out-of-table foreground, soft edges.
#[test]
fn paint_smudge_stays_in_the_table() {
    assert_tool("strict black and white", halves, "paint.smudge");
    assert_tool("per-column gradient", gradient, "paint.smudge");
}
/// paint.dodge: both palettes, out-of-table foreground, soft edges.
#[test]
fn paint_dodge_stays_in_the_table() {
    assert_tool("strict black and white", halves, "paint.dodge");
    assert_tool("per-column gradient", gradient, "paint.dodge");
}
/// paint.burn: both palettes, out-of-table foreground, soft edges.
#[test]
fn paint_burn_stays_in_the_table() {
    assert_tool("strict black and white", halves, "paint.burn");
    assert_tool("per-column gradient", gradient, "paint.burn");
}
/// paint.sponge: both palettes, out-of-table foreground, soft edges.
#[test]
fn paint_sponge_stays_in_the_table() {
    assert_tool("strict black and white", halves, "paint.sponge");
    assert_tool("per-column gradient", gradient, "paint.sponge");
}
/// edit.paste: both palettes, out-of-table foreground, soft edges.
#[test]
fn edit_paste_stays_in_the_table() {
    assert_tool("strict black and white", halves, "edit.paste");
    assert_tool("per-column gradient", gradient, "edit.paste");
}
/// image.rotation.arbitrary bilinear: both palettes, out-of-table foreground, soft edges.
#[test]
fn image_rotation_arbitrary_bilinear_stays_in_the_table() {
    assert_tool("strict black and white", halves, "image.rotation.arbitrary bilinear");
    assert_tool("per-column gradient", gradient, "image.rotation.arbitrary bilinear");
}
