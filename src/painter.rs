use crate::{model::*, session::Editor};
use eframe::egui::{self, *};
use std::collections::BTreeMap;
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    for (id, data) in FONT_FILES {
        fonts
            .font_data
            .insert((*id).into(), FontData::from_static(data).into());
        fonts
            .families
            .insert(FontFamily::Name((*id).into()), vec![(*id).into()]);
    }
    ctx.set_fonts(fonts);
}
pub fn color(s: &str) -> Color32 {
    let n = u32::from_str_radix(&s[1..], 16).unwrap();
    Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8)
}
// epaint 0.33 converts font sizes directly from em units (FontExt::px_scale_factor).
pub fn font(style: &crate::model::TextStyle) -> FontId {
    FontId::new(
        style.font_size as f32,
        FontFamily::Name(style.font_id.clone().into()),
    )
}
pub fn static_text(p: &Painter, n: &Node, origin: Pos2) {
    type Fragment<'a> = (&'a str, f64, f64, Option<&'a ResolvedRun>, Option<&'a Box2>);
    for l in &n.lines {
        let fragments: Vec<Fragment<'_>> = match &l.runs {
            Some(runs) => runs
                .iter()
                .map(|r| {
                    (
                        r.text.as_str(),
                        r.text_x,
                        r.baseline,
                        r.resolved.as_ref(),
                        Some(&r.r#box),
                    )
                })
                .collect(),
            None => vec![(&l.text, l.text_x, l.baseline, None, None)],
        };
        for (text, left, supplied_baseline, resolved, bounds) in fragments {
            let style = resolved.map(|r| &r.text_style).unwrap_or(&n.text_style);
            let ink = color(
                resolved
                    .map(|r| r.foreground.as_str())
                    .unwrap_or(&n.paint.foreground),
            );
            if let (Some(h), Some(b)) = (resolved.and_then(|r| r.highlight.as_ref()), bounds) {
                let rect = Rect::from_min_size(
                    origin + vec2(b.x as f32, b.y as f32),
                    vec2(b.width as f32, b.height as f32),
                );
                p.rect_filled(rect, h.radius as u8, color(&h.fill));
                p.rect_stroke(
                    rect,
                    h.radius as u8,
                    Stroke::new(h.width as f32, color(&h.stroke)),
                    StrokeKind::Inside,
                );
            }
            let data = FONT_FILES
                .iter()
                .find(|(id, _)| *id == style.font_id)
                .unwrap()
                .1;
            let face = ttf_parser::Face::parse(data, 0).unwrap();
            let scale = style.font_size / face.units_per_em() as f64;
            let mut x = left;
            for c in text.chars() {
                let g = p.layout_no_wrap(c.to_string(), font(style), ink);
                let baseline = g
                    .rows
                    .first()
                    .and_then(|r| r.glyphs.first().map(|g| r.pos.y + g.pos.y))
                    .unwrap_or(0.);
                p.galley(
                    origin + vec2(x as f32, supplied_baseline as f32 - baseline),
                    g,
                    ink,
                );
                x += face
                    .glyph_index(c)
                    .and_then(|g| face.glyph_hor_advance(g))
                    .unwrap_or(0) as f64
                    * scale
                    + style.letter_spacing;
            }
            if let (Some(u), Some(b)) = (resolved.and_then(|r| r.underline.as_ref()), bounds) {
                p.line_segment(
                    [
                        origin + vec2(left as f32, u.y as f32),
                        origin + vec2((left + b.width) as f32, u.y as f32),
                    ],
                    Stroke::new(u.width as f32, color(&u.color)),
                );
            }
        }
    }
}
/// Returns a finalized control, never a role-derived action.
pub fn surface(
    ui: &mut Ui,
    e: &mut Editor,
    selections: &BTreeMap<String, crate::transport::SelectionCapture>,
) -> (Option<Control>, Option<String>) {
    let Some(presentation) = e.presentation.clone() else {
        return (None, None);
    };
    let (_, action, inspected) = surface_inner(ui, &presentation, Some(e), selections);
    (action, inspected)
}

/// Paint a validated, frozen presentation with the production paint path.
/// Inputs show received values without editing/focus; no action widgets exist.
pub fn snapshot(ui: &mut Ui, presentation: &Presentation) -> Rect {
    surface_inner(ui, presentation, None, &BTreeMap::new()).0
}

fn surface_inner(
    ui: &mut Ui,
    presentation: &Presentation,
    mut editor: Option<&mut Editor>,
    selections: &BTreeMap<String, crate::transport::SelectionCapture>,
) -> (Rect, Option<Control>, Option<String>) {
    let Some(d) = presentation.display.as_ref() else {
        return (Rect::NOTHING, None, None);
    };
    let actionable = editor.as_ref().is_some_and(|e| e.can_act());
    let (rect, _) = ui.allocate_exact_size(vec2(d.width as f32, d.height as f32), Sense::hover());
    let origin = rect.min;
    let mut action = None;
    let mut inspected = None;
    let mut clips = BTreeMap::<String, Rect>::new();
    let mut curved = BTreeMap::<String, Vec<Vec<Pos2>>>::new();
    for n in &d.nodes {
        let b = &n.r#box;
        let r = Rect::from_min_size(
            origin + vec2(b.x as f32, b.y as f32),
            vec2(b.width as f32, b.height as f32),
        );
        let parent_clip = n
            .parent_id
            .as_ref()
            .and_then(|id| clips.get(id))
            .copied()
            .unwrap_or(ui.clip_rect());
        let clip = if n.paint.clip {
            parent_clip.intersect(r)
        } else {
            parent_clip
        };
        clips.insert(n.id.clone(), clip);
        let mut polygons = n
            .parent_id
            .as_ref()
            .and_then(|id| curved.get(id))
            .cloned()
            .unwrap_or_default();
        let parent_polygons = polygons.clone();
        let boundary = crate::paint_geometry::outline(r, &n.paint.corners);
        if n.paint.clip
            && (n.paint.corners.kind == "ellipse" || n.paint.corners.radius.unwrap_or(0.) > 0.)
        {
            polygons.push(boundary.clone());
        }
        // Paint order resolves overlapping nodes; clipping also applies to inspection.
        // Secondary clicks never dispatch a domain action or edit a working input.
        if editor.is_some()
            && ui.rect_contains_pointer(r.intersect(clip))
            && ui.input(|i| i.pointer.hover_pos()).is_some_and(|p| {
                polygons
                    .iter()
                    .all(|v| crate::paint_geometry::contains(p, v))
            })
            && ui.input(|i| i.pointer.secondary_clicked())
        {
            inspected = Some(n.id.clone());
        }
        curved.insert(n.id.clone(), polygons.clone());
        let start = ui
            .ctx()
            .graphics_mut(|g| g.entry(ui.layer_id()).next_idx().0);
        let painter = ui.painter().with_clip_rect(clip);
        let radius = n
            .paint
            .corners
            .radius
            .unwrap_or(0.)
            .min(b.width.min(b.height) / 2.) as f32;
        match n.paint.background.kind.as_str() {
            "solid" => {
                painter.add(Shape::convex_polygon(
                    boundary.clone(),
                    color(n.paint.background.color.as_ref().unwrap()),
                    Stroke::NONE,
                ));
            }
            "linear-gradient" => {
                painter.add(Shape::mesh(crate::paint_geometry::clip(
                    &crate::paint_geometry::gradient(r, &n.paint.background),
                    &boundary,
                )));
            }
            _ => {}
        }
        if n.paint.border.width > 0. {
            let width = n.paint.border.width as f32;
            let border = crate::paint_geometry::outline(
                r.shrink(width / 2.),
                &crate::model::Corners {
                    kind: n.paint.corners.kind.clone(),
                    radius: Some((radius - width / 2.).max(0.) as f64),
                },
            );
            painter.add(Shape::closed_line(
                border,
                Stroke::new(width, color(&n.paint.border.color)),
            ));
        }
        if let Some(src) = &n.src {
            match crate::images::texture(ui.ctx(), src) {
                Ok(Some(texture)) => {
                    let size = texture.size_vec2();
                    let scale = (r.width() / size.x).max(r.height() / size.y);
                    let fraction = r.size() / (size * scale);
                    let uv = Rect::from_center_size(pos2(0.5, 0.5), fraction);
                    let mut mesh = epaint::Mesh::with_texture(texture.id());
                    mesh.add_rect_with_uv(r, uv, Color32::WHITE);
                    painter.add(Shape::mesh(crate::paint_geometry::clip(&mesh, &boundary)));
                }
                Ok(None) => {}
                Err(error) => {
                    ui.interact(r, ui.id().with((&n.id, "image-error")), Sense::hover())
                        .on_hover_text(format!("Image unavailable: {error}"));
                }
            }
        }
        let content_start = ui
            .ctx()
            .graphics_mut(|g| g.entry(ui.layer_id()).next_idx().0);
        let mut state_outline = None;
        if n.kind == "input" {
            let c = presentation
                .controls
                .iter()
                .find(|c| c.id == n.id && c.binding.presented)
                .unwrap();
            let slot = &c.binding.slot;
            let mut value = editor
                .as_ref()
                .map(|e| {
                    e.fields
                        .get(slot)
                        .map(|f| f.value.clone())
                        .unwrap_or_default()
                })
                .unwrap_or_else(|| n.payload.clone());
            let input_id = editor
                .as_ref()
                .map(|e| Id::new((e.id, slot)))
                .unwrap_or_else(|| ui.id().with(("snapshot-input", &n.id)));
            let inner = r.shrink2(vec2(
                (n.text_style.inset_x / 2.).min(b.width / 2.) as f32,
                (n.text_style.inset_y / 2.).min(b.height / 2.) as f32,
            ));
            let out = ui
                .scope_builder(UiBuilder::new().max_rect(inner), |ui| {
                    ui.set_clip_rect(clip.intersect(r));
                    if editor.is_some()
                        && ui.input(|i| {
                            i.pointer.any_pressed()
                                && i.pointer.hover_pos().is_some_and(|p| {
                                    !polygons
                                        .iter()
                                        .all(|v| crate::paint_geometry::contains(p, v))
                                })
                        })
                    {
                        ui.disable();
                    }
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    ui.add_sized(
                        inner.size(),
                        TextEdit::singleline(&mut value)
                            .id(input_id)
                            .interactive(editor.is_some())
                            .font(font(&n.text_style))
                            .text_color(color(&n.paint.foreground))
                            .frame(false)
                            .margin(Vec2::ZERO)
                            .vertical_align(Align::Center)
                            .horizontal_align(if presentation.context.direction == "rtl" {
                                Align::RIGHT
                            } else {
                                Align::LEFT
                            })
                            .desired_width(inner.width()),
                    )
                })
                .inner;
            out.ctx
                .accesskit_node_builder(out.id, |node| node.set_label(n.semantics.label.clone()));
            if out.changed()
                && let Some(e) = editor.as_deref_mut()
            {
                e.edit(slot, value);
            }
            if out.has_focus()
                && let Some(o) = &n.paint.states.focus
            {
                state_outline = Some(o);
            }
        } else {
            static_text(&painter, n, origin);
            // Descriptive link semantics only: no navigation or Goo invocation is derived here.
            for (line_index, line) in n.lines.iter().enumerate() {
                for (run_index, run) in line.runs.iter().flatten().enumerate() {
                    if let Some(target) = run
                        .resolved
                        .as_ref()
                        .and_then(|r| r.semantics.target.as_ref())
                    {
                        let bounds = &run.r#box;
                        let rect = Rect::from_min_size(
                            origin + vec2(bounds.x as f32, bounds.y as f32),
                            vec2(bounds.width as f32, bounds.height as f32),
                        );
                        let response = ui.interact(
                            rect.intersect(clip),
                            ui.id().with(("run-link", &n.id, line_index, run_index)),
                            Sense::hover(),
                        );
                        response.widget_info(|| {
                            WidgetInfo::labeled(WidgetType::Link, false, &run.text)
                        });
                        let label = match target {
                            RunLink::Path { path } => path,
                            RunLink::Weblink { href } => href,
                        };
                        response.on_hover_text(label);
                    }
                }
            }
            if let (Some(marker), Some(line)) = (&n.paint.marker, n.lines.first()) {
                let diameter = marker["diameter"].as_f64().unwrap();
                let x = if presentation.context.direction == "rtl" {
                    line.r#box.x + line.r#box.width - diameter / 2.
                } else {
                    line.text_x - line.prefix_advance + diameter / 2.
                };
                painter.circle_filled(
                    origin + vec2(x as f32, (line.baseline - diameter / 2.) as f32),
                    diameter as f32 / 2.,
                    color(&n.paint.foreground),
                );
            }
            if n.kind != "group"
                && let Some(e) = editor.as_ref()
            {
                let c = presentation.controls.iter().find(|c| {
                    c.id == n.id
                        && c.binding.presented
                        && ["invoke", "become", "invoke-and-become"]
                            .contains(&c.binding.kind.as_str())
                });
                let enabled = actionable
                    && c.is_some_and(|c| {
                        c.binding.captures.iter().all(|r| {
                            selections
                                .get(&r.selection)
                                .is_some_and(|v| v.member.status == "available")
                        })
                    });
                let response = ui.interact(
                    r.intersect(clip),
                    Id::new((e.id, &n.id)),
                    if enabled {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                );
                response.widget_info(|| {
                    WidgetInfo::labeled(
                        if n.kind == "button" {
                            WidgetType::Button
                        } else if n.semantics.role == "image" {
                            WidgetType::Image
                        } else if n.semantics.role == "progressbar" {
                            WidgetType::ProgressIndicator
                        } else {
                            WidgetType::Label
                        },
                        enabled,
                        n.semantics.label.clone(),
                    )
                });
                response.ctx.accesskit_node_builder(response.id, |node| {
                    if let Some(range) = &n.semantics.range {
                        node.set_min_numeric_value(range.minimum);
                        node.set_max_numeric_value(range.maximum);
                        node.set_numeric_value(range.value);
                    }
                });
                if enabled
                    && ((response.clicked()
                        && response.interact_pointer_pos().is_none_or(|p| {
                            polygons
                                .iter()
                                .all(|v| crate::paint_geometry::contains(p, v))
                        }))
                        || (response.has_focus()
                            && ui
                                .input(|i| i.key_pressed(Key::Enter) || i.key_pressed(Key::Space))))
                {
                    action = c.cloned();
                }
                let outline = if response.has_focus() {
                    n.paint.states.focus.as_ref()
                } else if response.is_pointer_button_down_on() {
                    n.paint.states.highlight.as_ref()
                } else {
                    None
                };
                if let Some(o) = outline {
                    state_outline = Some(o);
                }
            }
        }
        crate::paint_geometry::clip_paint(ui, content_start, &polygons);
        if let Some(o) = state_outline {
            ui.painter().with_clip_rect(parent_clip).rect_stroke(
                r.expand(o.offset as f32 + o.width as f32 / 2.),
                radius,
                Stroke::new(o.width as f32, color(&o.color)),
                StrokeKind::Middle,
            );
        }
        crate::paint_geometry::clip_paint(ui, start, &parent_polygons);
    }
    if inspected.is_some() {
        action = None;
    }
    (rect, action, inspected)
}
