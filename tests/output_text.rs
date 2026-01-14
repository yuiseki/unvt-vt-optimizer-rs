use vt_optimizer::mbtiles::TileSummary;
use vt_optimizer::output::format_tile_summary_text;

#[test]
fn format_tile_summary_text_includes_tile_counts() {
    let summary = TileSummary {
        zoom: 12,
        x: 345,
        y: 678,
        layer_count: 3,
        total_features: 42,
        vertex_count: 9001,
        property_key_count: 7,
        property_value_count: 9,
        layers: Vec::new(),
    };

    let lines = format_tile_summary_text(&summary);

    assert_eq!(
        lines,
        vec![
            "- z=12 x=345 y=678".to_string(),
            "- Layers in this tile: 3".to_string(),
            "- Features in this tile: 42".to_string(),
            "- Vertices in this tile: 9001".to_string(),
            "- Keys in this tile: 7".to_string(),
            "- Values in this tile: 9".to_string(),
        ]
    );
}
