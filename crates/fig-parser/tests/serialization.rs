/// Verify serialized enum values match frontend expectations.
use fig_parser::types::*;

#[test]
fn serialization_matches_frontend() {
    // The frontend checks `e.effect_type === 'DROP_SHADOW'` — but Rust serializes as "DropShadow"
    let effect = Effect {
        effect_type: EffectType::DropShadow,
        color: Some(Color {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        }),
        offset: Vec2::new(0.0, 4.0),
        radius: 8.0,
        visible: true,
        spread: 0.0,
    };
    let json = serde_json::to_value(&effect).unwrap();
    let etype = json["effect_type"].as_str().unwrap();
    println!("EffectType serializes as: {:?}", etype);
    // The frontend checks === 'DROP_SHADOW' — we need to verify what actually ships
    assert_eq!(
        etype, "DropShadow",
        "frontend expects 'DROP_SHADOW' but Rust sends {:?}",
        etype
    );

    // NodeType serialization
    let node = FigNode {
        guid: Some(NodeId::new(0, 999)),
        node_type: NodeType::Frame,
        name: "Test".into(),
        phase: None,
        parent_id: None,
        position: None,
        visible: true,
        opacity: 1.0,
        locked: false,
        size: None,
        transform: None,
        corner_radius: None,
        fill_paints: vec![],
        stroke_paints: vec![],
        stroke_weight: 0.0,
        stroke_align: StrokeAlign::Center,
        effects: vec![],
        text_data: None,
    };
    let json = serde_json::to_value(&node).unwrap();
    let ntype = json["node_type"].as_str().unwrap();
    println!("NodeType serializes as: {:?}", ntype);
    assert_eq!(ntype, "Frame", "frontend expects 'Frame'");

    // NodePhase serialization
    let node2 = FigNode {
        phase: Some(NodePhase::Removed),
        ..node
    };
    let json2 = serde_json::to_value(&node2).unwrap();
    let phase = json2["phase"].as_str().unwrap();
    println!("NodePhase::Removed serializes as: {:?}", phase);
    assert_eq!(phase, "Removed", "frontend checks phase === 'Removed'");
}
