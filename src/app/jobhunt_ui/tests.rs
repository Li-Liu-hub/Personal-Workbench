use super::*;
use crate::app::editor_tests::{input_frame, pointer};
fn position(output: &egui::FullOutput, label: &str) -> Option<egui::Pos2> {
    output.shapes.iter().find_map(|s| match &s.shape {
        egui::Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() * 0.5),
        _ => None,
    })
}
#[test]
fn jobhunt_navigation_and_tabs_show_shared_records_in_both_views() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    let mut site = Site {
        company: "样例公司".into(),
        position: "Java实习生".into(),
        url: "https://jobs.example.com/1".into(),
        ..Site::new()
    };
    app.db
        .as_mut()
        .unwrap()
        .save_jobhunt_site(&mut site)
        .unwrap();
    crate::hiring::store::Store::open(&dir.path().join("hiring")).unwrap();
    let conn = rusqlite::Connection::open(dir.path().join("hiring/hiring.sqlite")).unwrap();
    conn.execute("INSERT INTO attempts(site,url,title,company,at,day,status,note) VALUES('official','https://jobs.example.com/1','Java实习生','样例公司',1,'2026-10-27','ambiguous','')",[]).unwrap();
    for mode in [ViewMode::Grid, ViewMode::List] {
        let ctx = egui::Context::default();
        configure_theme(&ctx);
        app.section = Section::JobHunt;
        app.set_view_mode(mode);
        app.jobhunt.tab = 0;
        input_frame(&mut app, &ctx, 0., vec![], vec![]);
        let out = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
        assert!(position(&out, "样例公司").is_some());
        assert!(position(&out, "待核验").is_some());
        assert!(position(&out, "网站").is_some());
        let p = position(&out, "投递记录").unwrap();
        input_frame(&mut app, &ctx, 0.2, pointer(p, true), vec![]);
        input_frame(&mut app, &ctx, 0.3, pointer(p, false), vec![]);
        assert_eq!(app.jobhunt.tab, 1);
        let out = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
        assert!(position(&out, "待核验").is_some());
        app.search_query = "无匹配关键词".into();
        let out = input_frame(&mut app, &ctx, 0.5, vec![], vec![]);
        assert!(position(&out, "样例公司").is_none());
        app.search_query.clear();
    }
}

#[test]
fn jobhunt_website_editor_saves_and_delete_requires_confirmation() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.section = Section::JobHunt;
    app.add_current_item();
    assert!(app.detail_open());
    let draft = app.jobhunt.draft.as_mut().unwrap();
    draft.company = "记录公司".into();
    draft.position = "后端实习生".into();
    draft.url = "https://example.com/jobs/1".into();
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    let mut clock = 0.;
    fn click(app: &mut PersonalDeskApp, ctx: &egui::Context, clock: &mut f64, label: &str) {
        *clock += 0.1;
        input_frame(app, ctx, *clock, vec![], vec![]);
        *clock += 0.1;
        let out = input_frame(app, ctx, *clock, vec![], vec![]);
        let pos = position(&out, label).unwrap_or_else(|| panic!("button missing: {label}"));
        *clock += 0.1;
        input_frame(app, ctx, *clock, pointer(pos, true), vec![]);
        *clock += 0.1;
        input_frame(app, ctx, *clock, pointer(pos, false), vec![]);
    }
    click(&mut app, &ctx, &mut clock, "保存网站记录");
    assert_eq!(app.db.as_ref().unwrap().jobhunt_sites().unwrap().len(), 1);
    assert!(app.jobhunt.draft.is_none());
    click(&mut app, &ctx, &mut clock, "编辑");
    app.jobhunt.draft.as_mut().unwrap().notes = "适合投递的岗位".into();
    click(&mut app, &ctx, &mut clock, "保存网站记录");
    assert_eq!(
        app.db.as_ref().unwrap().jobhunt_sites().unwrap()[0].notes,
        "适合投递的岗位"
    );
    click(&mut app, &ctx, &mut clock, "删除");
    assert!(app.jobhunt.deleting.is_some());
    assert_eq!(app.db.as_ref().unwrap().jobhunt_sites().unwrap().len(), 1);
    click(&mut app, &ctx, &mut clock, "确认删除记录");
    assert!(app.db.as_ref().unwrap().jobhunt_sites().unwrap().is_empty());
}

#[test]
fn jobhunt_card_actions_remain_horizontal_in_small_and_standard_windows() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.section = Section::JobHunt;
    let mut site = Site {
        company: "布局公司".into(),
        position: "Java后端开发实习生（2027届校招）".into(),
        url: "https://jobs.example.com/very/long/application/url".into(),
        location: "北京 上海 深圳".into(),
        ..Site::new()
    };
    app.db
        .as_mut()
        .unwrap()
        .save_jobhunt_site(&mut site)
        .unwrap();
    let mut second = Site {
        company: "第二公司".into(),
        position: "Java研发".into(),
        url: "https://example.com/2".into(),
        ..Site::new()
    };
    app.db
        .as_mut()
        .unwrap()
        .save_jobhunt_site(&mut second)
        .unwrap();
    for size in [[1000., 680.], [1280., 800.]] {
        for mode in [ViewMode::Grid, ViewMode::List] {
            let ctx = egui::Context::default();
            configure_theme(&ctx);
            app.set_view_mode(mode);
            let mut out = egui::FullOutput::default();
            for _ in 0..3 {
                out = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size.into())),
                        ..Default::default()
                    },
                    |ui| app.render_workspace(ui),
                );
                out.textures_delta.clear();
            }
            let actions: Vec<_> = out
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    egui::Shape::Text(t)
                        if t.galley.text() == "打开网站"
                            || t.galley.text() == "编辑"
                            || t.galley.text() == "删除" =>
                    {
                        Some(t)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(actions.len(), 6);
            assert!(
                actions.iter().all(|t| t.galley.rows.len() == 1),
                "wrapped card actions in {size:?}"
            );
            let ys: Vec<_> = actions
                .iter()
                .map(|t| t.pos.y + t.galley.size().y * 0.5)
                .collect();
            assert!(
                ys.chunks(3)
                    .all(|group| group.iter().all(|y| (y - group[0]).abs() < 4.)),
                "actions stacked in {size:?}, mode {mode:?}, positions {ys:?}"
            );
            let first = position(&out, "布局公司").unwrap();
            let second = position(&out, "第二公司").unwrap();
            if mode == ViewMode::Grid {
                assert!(
                    (first.y - second.y).abs() < 3.,
                    "grid cards must align at top"
                );
            } else {
                assert!((first.x - second.x).abs() < 3. && second.y > first.y + 60.);
            }
        }
    }
}

#[test]
fn hackathon_tab_displays_saved_competitions_and_registration() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = PersonalDeskApp::open_data_directory(dir.path().into());
    app.db.as_mut().unwrap().set_setting("hackathon.events", r#"[{"title":"示例黑客松","details":"线上比赛","url":"https://example.org/source","official_url":"https://example.org/register","source":"测试赛事","collected":"2026-10-29"}]"#).unwrap();
    app.section = Section::JobHunt;
    let ctx = egui::Context::default();
    configure_theme(&ctx);
    input_frame(&mut app, &ctx, 0., vec![], vec![]);
    let output = input_frame(&mut app, &ctx, 0.1, vec![], vec![]);
    let p = position(&output, "黑客松比赛").unwrap();
    input_frame(&mut app, &ctx, 0.2, pointer(p, true), vec![]);
    input_frame(&mut app, &ctx, 0.3, pointer(p, false), vec![]);
    let output = input_frame(&mut app, &ctx, 0.4, vec![], vec![]);
    assert!(position(&output, "示例黑客松").is_some());
    assert!(position(&output, "报名 / 比赛网站").is_some());
}
