//! Styling for the native host shell. Prepared Goo paint stays in `painter`.
use eframe::egui::*;

pub const BACKGROUND: Color32 = Color32::from_rgb(241, 244, 246);
pub const BORDER: Color32 = Color32::from_rgb(216, 225, 229);
pub const INK: Color32 = Color32::from_rgb(35, 55, 64);
pub const MUTED: Color32 = Color32::from_rgb(99, 119, 129);
pub const ACCENT: Color32 = Color32::from_rgb(21, 113, 112);

pub fn install(ctx: &Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = Visuals::light();
    style.visuals.panel_fill = BACKGROUND;
    style.visuals.window_fill = Color32::WHITE;
    style.visuals.faint_bg_color = BACKGROUND;
    style.visuals.extreme_bg_color = Color32::WHITE;
    style.visuals.override_text_color = Some(INK);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1_f32, BORDER);
    style.visuals.widgets.noninteractive.bg_fill = BACKGROUND;
    style.visuals.widgets.noninteractive.fg_stroke = Stroke::new(1_f32, MUTED);
    style.visuals.selection.bg_fill = Color32::from_rgb(215, 238, 234);
    style.visuals.selection.stroke = Stroke::new(1_f32, ACCENT);
    for widgets in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
    ] {
        widgets.corner_radius = CornerRadius::same(6);
        widgets.fg_stroke = Stroke::new(1_f32, INK);
    }
    style.visuals.widgets.inactive.bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.weak_bg_fill = Color32::WHITE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1_f32, BORDER);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(232, 242, 241);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(232, 242, 241);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1_f32, ACCENT);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(215, 238, 234);
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(215, 238, 234);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1_f32, ACCENT);
    style.spacing.item_spacing = vec2(8., 8.);
    style.spacing.button_padding = vec2(10., 6.);
    style.spacing.interact_size.y = 28.;
    style.spacing.indent = 18.;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(13.));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(13.));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(11.));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::proportional(18.));
    // eframe applies the OS theme after creation; install the intended light
    // style explicitly so the first native focus event cannot replace it.
    ctx.set_theme(ThemePreference::Light);
    ctx.set_style_of(Theme::Light, style);
}

pub fn panel() -> Frame {
    Frame::new()
        .fill(Color32::WHITE)
        .stroke(Stroke::new(1_f32, BORDER))
        .corner_radius(10)
        .inner_margin(16)
}

/// A pane owns one scroll viewport, bounded by its allocated workspace slot.
/// Its title stays fixed; content can grow without growing the workspace.
pub fn scroll_pane<R>(
    ui: &mut Ui,
    id: &str,
    title: &str,
    contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<scroll_area::ScrollAreaOutput<R>> {
    panel().show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        ui.label(RichText::new(title).size(16.).strong());
        ui.separator();
        ScrollArea::both()
            .id_salt((id, "pane-scroll"))
            .auto_shrink([false, false])
            .min_scrolled_height(0.)
            .max_height(ui.available_height().max(0.))
            .show(ui, contents)
    })
}

/// Register the row before its contents so disclosure buttons and any prepared
/// controls keep their own hit targets. Selection belongs to the tree host.
pub fn tree_row<R>(
    ui: &mut Ui,
    label: &str,
    selected: bool,
    selectable: bool,
    height: f32,
    contents: impl FnOnce(&mut Ui) -> R,
) -> InnerResponse<R> {
    let (rect, _) =
        ui.allocate_exact_size(vec2(ui.available_width(), height.max(28.)), Sense::hover());
    let response = ui.interact(
        rect,
        ui.id().with("select-row"),
        if selectable {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    response.widget_info(|| {
        WidgetInfo::selected(WidgetType::SelectableLabel, selectable, selected, label)
    });
    if selected || response.hovered() || response.has_focus() {
        ui.painter().rect_filled(
            rect,
            5.,
            if selected {
                ui.visuals().selection.bg_fill
            } else {
                BACKGROUND
            },
        );
    }
    if response.has_focus() {
        ui.painter()
            .rect_stroke(rect, 5., Stroke::new(1_f32, ACCENT), StrokeKind::Inside);
    }
    let inner = ui
        .scope_builder(UiBuilder::new().max_rect(rect.shrink(4.)), contents)
        .inner;
    InnerResponse {
        inner,
        response: response.on_hover_text(label),
    }
}

pub fn disclosure(ui: &mut Ui, open: bool) -> Response {
    let (_, response) = ui.allocate_exact_size(vec2(20., 20.), Sense::click());
    // Paint the standard egui triangle: a font glyph is not portable across
    // the native shell's font set.
    let mut icon = response.clone();
    icon.rect = Rect::from_center_size(response.rect.center(), vec2(10., 10.));
    collapsing_header::paint_default_icon(ui, if open { 1. } else { 0. }, &icon);
    let label = if open {
        "Collapse branch"
    } else {
        "Expand branch"
    };
    response.widget_info(|| WidgetInfo::labeled(WidgetType::Button, true, label));
    response.on_hover_text(label)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_scroll_stays_bounded_and_does_not_move_its_neighbor_or_header() {
        let ctx = Context::default();
        install(&ctx);
        let frame = |events| {
            let mut offsets = [Vec2::ZERO; 2];
            let mut panes = [Rect::NOTHING; 2];
            let mut header = Rect::NOTHING;
            let mut bounds = Rect::NOTHING;
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900., 500.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| {
                        header = ui.label("Fixed workspace navigation").rect;
                        bounds = ui.available_rect_before_wrap();
                        ui.columns(2, |columns| {
                            for (index, ui) in columns.iter_mut().enumerate() {
                                let pane = scroll_pane(ui, &index.to_string(), "Pane", |ui| {
                                    // Oversized prepared content must not expand its viewport.
                                    ui.allocate_exact_size(vec2(1000., 2000.), Sense::hover());
                                });
                                offsets[index] = pane.inner.state.offset;
                                panes[index] = pane.response.rect;
                            }
                        });
                    });
                },
            );
            (header, bounds, panes, offsets)
        };
        frame(vec![]);
        let (header, bounds, panes, _) = frame(vec![]);
        for pane in panes {
            assert!(
                pane.bottom() <= bounds.bottom() + 1.,
                "{pane:?} exceeds {bounds:?}"
            );
        }
        let scroll = |pos| {
            vec![
                Event::PointerMoved(pos),
                Event::MouseWheel {
                    unit: MouseWheelUnit::Point,
                    delta: vec2(0., -10000.),
                    modifiers: Modifiers::default(),
                },
            ]
        };
        frame(scroll(panes[0].center()));
        for _ in 0..20 {
            frame(vec![]);
        }
        let (after_header, _, after_panes, offsets) = frame(scroll(panes[0].center()));
        assert_eq!(after_header, header);
        assert_eq!(after_panes, panes);
        assert!(offsets[0].y > 0.);
        assert_eq!(offsets[1], Vec2::ZERO);
    }

    #[test]
    fn tree_selection_does_not_steal_disclosure_or_prepared_control_clicks() {
        let ctx = Context::default();
        install(&ctx);
        let frame = |events: Vec<Event>, selectable| {
            let mut result = (false, false, false);
            let mut targets = [Pos2::ZERO; 3];
            let _ = ctx.run(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(400., 120.))),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    CentralPanel::default().show(ctx, |ui| {
                        let row = tree_row(ui, "/example", false, selectable, 40., |ui| {
                            ui.horizontal(|ui| {
                                let disclosure = disclosure(ui, false);
                                let label = ui.add(Label::new("Prepared label").selectable(false));
                                let action = ui.button("Prepared action");
                                targets = [
                                    disclosure.rect.center(),
                                    label.rect.center(),
                                    action.rect.center(),
                                ];
                                result.1 = disclosure.clicked();
                                result.2 = action.clicked();
                            });
                        });
                        result.0 = row.response.clicked();
                    });
                },
            );
            (result, targets)
        };
        frame(vec![], true);
        let (_, targets) = frame(vec![], true);
        let click = |pos, selectable| {
            frame(
                vec![
                    Event::PointerMoved(pos),
                    Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed: true,
                        modifiers: Modifiers::default(),
                    },
                ],
                selectable,
            );
            frame(
                vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::default(),
                }],
                selectable,
            )
            .0
        };
        assert_eq!(click(targets[0], true), (false, true, false));
        assert_eq!(click(targets[1], true), (true, false, false));
        assert_eq!(click(targets[2], true), (false, false, true));
        frame(vec![], false);
        assert_eq!(click(targets[1], false), (false, false, false));
    }
}
