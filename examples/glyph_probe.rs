// Headless check: which of the UI glyphs exist in egui's default fonts?
// Run: cargo run --example glyph_probe
fn main() {
    let ctx = egui::Context::default();
    let _ = ctx.run_ui(Default::default(), |ui| {
        ui.ctx().fonts_mut(|f| {
            let font_id = egui::FontId::proportional(14.0);
            for c in "⏺✎✐⚑⚐★☆◆◇".chars() {
                let ok = f.has_glyph(&font_id, c);
                println!("{} U+{:05X} {}", if ok { "OK  " } else { "MISS" }, c as u32, c);
            }
        });
    });
}
