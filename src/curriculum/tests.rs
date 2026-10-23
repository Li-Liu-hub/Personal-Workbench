use super::*;
fn candidates() -> Vec<Candidate> {
    let mut cards = BTreeMap::new();
    for text in [
        include_str!("../../assets/engineering.json"),
        include_str!("../../assets/sql_practice.json"),
        include_str!("../../assets/foundations.json"),
        include_str!("../../assets/reference_practice.json"),
    ] {
        for card in serde_json::from_str::<Vec<Card>>(text).unwrap() {
            cards.insert(card.id.clone(), card);
        }
    }
    let card = crate::challenges::card();
    cards.insert(card.id.clone(), card);
    cards
        .into_values()
        .map(|card| Candidate {
            training: profile(&card),
            card,
            independent: 0,
            latest: None,
            mastered: false,
        })
        .collect()
}
#[test]
fn complete_route_covers_seven_categories_eight_abilities_and_solvable_sql() {
    let items = candidates();
    assert_eq!(items.len(), BUILTIN_COUNT as usize);
    for item in &items {
        item.card.validate().unwrap();
        item.training.validate().unwrap();
        assert!(crate::study::TOPICS.contains(&category(&item.card.topic)));
        if let Some(lab) = &item.card.sql
            && lab.runnable
        {
            let report = crate::sql_practice::run(lab, &lab.reference);
            assert_eq!(
                report.matches,
                Some(true),
                "{}: {:?}",
                item.card.id,
                report.error
            );
            assert!(report.rolled_back);
        }
    }
    for area in AREAS {
        for level in 1..=2 {
            assert!(
                items
                    .iter()
                    .filter(|r| r.training.area == area && r.training.level == level)
                    .count()
                    >= 2,
                "{area} lacks level {level}"
            );
        }
    }
}
#[test]
fn recommendations_diagnose_all_areas_before_advanced_and_explain_choices() {
    let mut items = candidates();
    let rules = Rules::default();
    let mut recent = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for n in 0..8 {
        let pick = choose(&items, "", &Filter::default(), &recent, &rules, n).unwrap();
        assert!(pick.reason.contains("基础摸底"));
        let r = &mut items[pick.index];
        assert_eq!(r.training.level, 1);
        assert!(seen.insert(r.training.area.clone()));
        r.independent = 1;
        r.latest = Some(30);
        recent.push(r.card.id.clone());
    }
    assert_eq!(seen.len(), 8);
    for n in 0..100 {
        let pick = choose(&items, "", &Filter::default(), &[], &rules, n).unwrap();
        assert_eq!(items[pick.index].training.level, 1);
    }
}
#[test]
fn skipping_questions_still_samples_different_basic_directions() {
    let items = candidates();
    let mut recent = Vec::new();
    let mut areas = std::collections::BTreeSet::new();
    for n in 0..8 {
        let p = choose(
            &items,
            "",
            &Filter::default(),
            &recent,
            &Rules::default(),
            n,
        )
        .unwrap();
        areas.insert(items[p.index].training.area.clone());
        recent.push(items[p.index].card.id.clone());
    }
    assert_eq!(areas.len(), 8);
}
#[test]
fn promotion_needs_distinct_independent_passes_and_honors_filters() {
    let mut items = candidates();
    let rules = Rules::default();
    // Repeating one foundation cannot stand in for two different foundation questions.
    let first = items
        .iter_mut()
        .find(|r| r.training.area == AREAS[0] && r.training.level == 1)
        .unwrap();
    first.independent = 20;
    first.latest = Some(100);
    first.mastered = true;
    assert_eq!(route(&items, &rules)[0].stage, 1);
    for r in &mut items {
        if r.training.area == AREAS[0] && r.training.level == 1 {
            r.independent = 1;
            r.latest = Some(90);
        }
    }
    assert_eq!(route(&items, &rules)[0].stage, 2);
    let filter = Filter {
        area: AREAS[0].into(),
        level: 2,
    };
    let p = choose(&items, "", &filter, &[], &rules, 42).unwrap();
    assert_eq!(items[p.index].training.level, 2);
    let p = choose(&items, "项目实战", &Filter::default(), &[], &rules, 42).unwrap();
    assert!(
        items[p.index].card.project.is_some(),
        "manual specialization permits advanced projects"
    );
    assert!(
        choose(
            &items,
            "",
            &Filter {
                area: AREAS[5].into(),
                level: 3
            },
            &[],
            &rules,
            1
        )
        .is_none()
    );
    for r in &mut items {
        if r.training.area == AREAS[0] && r.training.level == 2 {
            r.independent = 1;
            r.latest = Some(90);
        }
    }
    assert_eq!(route(&items, &rules)[0].stage, 3);
}
#[test]
fn stopped_mastered_items_and_legacy_topic_aliases_are_respected() {
    let mut items = candidates();
    for r in &mut items {
        r.mastered = true;
    }
    let mut rules = Rules {
        mastered: MasteredMode::Stop,
        ..Default::default()
    };
    assert!(choose(&items, MIXED, &Filter::default(), &[], &rules, 0).is_none());
    rules.mastered = MasteredMode::Rare;
    for old in ["SQL 命令", "性能调优", "Bug 排查", "场景题", "项目修 Bug"] {
        let p = choose(&items, old, &Filter::default(), &[], &rules, 4).unwrap();
        assert_eq!(category(&items[p.index].card.topic), category(old));
    }
}
