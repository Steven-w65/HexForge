use hexforge_controller::preferences::PanelPreferences;
#[test]
fn panel_height_preserves_preference_and_clamps_for_small_windows() {
    let mut p = PanelPreferences::default();
    assert_eq!(p.effective_height(700.0), 220.0);
    p.preferred_height = 500.0;
    assert_eq!(p.effective_height(300.0), 136.0);
    assert_eq!(p.effective_height(190.0), 28.0);
    assert_eq!(p.bounds(210.0), (46.0, 46.0));
    assert_eq!(p.effective_height(1000.0), 500.0);
    p.collapsed = true;
    assert_eq!(p.effective_height(1000.0), 28.0);
}
#[test]
fn invalid_saved_height_uses_default_without_poisoning_geometry() {
    for raw in [
        None,
        Some(f32::NAN),
        Some(f32::INFINITY),
        Some(27.0),
        Some(-1.0),
    ] {
        assert_eq!(
            PanelPreferences::restore(raw, false).effective_height(700.0),
            220.0
        );
    }
    assert_eq!(
        PanelPreferences::restore(Some(28.0), false).effective_height(700.0),
        84.0
    );
}
#[test]
fn completion_notice_lasts_five_seconds_and_restarts_for_new_revision() {
    use hexforge_controller::preferences::NoticeClock;
    let mut clock = NoticeClock::default();
    assert_eq!(clock.update(1, true, 20.0), None);
    assert_eq!(clock.update(1, true, 24.999), None);
    assert_eq!(clock.update(1, true, 25.0), Some(1));
    assert_eq!(clock.update(2, true, 25.0), None);
    assert_eq!(clock.update(2, true, 29.999), None);
    assert_eq!(clock.update(2, true, 30.0), Some(2));
    clock.update(2, false, 30.0);
    assert_eq!(clock.remaining(31.0), None);
}
