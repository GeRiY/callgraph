use super::session_start::*;

fn facts() -> Facts {
    Facts {
        hook_installed: false,
        mcp: McpState::Missing,
        telemetry_open: true,
        update: None,
    }
}

fn all_good() -> Facts {
    Facts {
        hook_installed: true,
        mcp: McpState::Ok,
        telemetry_open: false,
        update: None,
    }
}

fn commands() -> Commands {
    Commands {
        exe: "\"/s/bin/linux-x86_64/callgraph\"".into(),
        current: "0.9.4".into(),
        update: "bash /s/callgraph.sh update".into(),
        hook_install: "\"/s/bin/linux-x86_64/callgraph\" hook install".into(),
        mcp_register: "claude mcp add -s user callgraph -- \"/s/bin/linux-x86_64/callgraph\" mcp --auto-update".into(),
        telemetry_on: "bash /s/callgraph.sh telemetry on".into(),
        telemetry_preview: "bash /s/callgraph.sh telemetry preview".into(),
    }
}

#[test]
fn nothing_to_say_prints_nothing() {
    let (items, next) = decide(&State::default(), &all_good(), 100);
    assert!(items.is_empty());
    assert_eq!(next, State::default());
    assert_eq!(render(&items, &all_good(), &commands()), None);
}

#[test]
fn shows_every_needed_item_then_waits_two_days_and_stops_after_three() {
    let mut state = State::default();
    let mut shown_days = Vec::new();
    for day in 100..106 {
        let (items, next) = decide(&state, &facts(), day);
        if !items.is_empty() {
            assert_eq!(items, vec![Item::Hook, Item::Mcp, Item::Telemetry]);
            shown_days.push(day);
        }
        state = next;
    }
    assert_eq!(shown_days, vec![100, 102, 104]);
}

/// Tételenkénti korlát: a hook és az MCP 3-szor, a telemetria 10-szer (mindegyik 2 naponta legfeljebb egyszer).
#[test]
fn per_item_limits_telemetry_up_to_ten_times_others_three() {
    assert_eq!(Item::Hook.max_shown(), 3);
    assert_eq!(Item::Mcp.max_shown(), 3);
    assert_eq!(Item::Telemetry.max_shown(), 10);
    let mut state = State::default();
    let mut shown: std::collections::BTreeMap<&str, Vec<i64>> = Default::default();
    for day in 100..140 {
        let (items, next) = decide(&state, &facts(), day);
        for item in items {
            shown.entry(item.key()).or_default().push(day);
        }
        state = next;
    }
    assert_eq!(shown["hook"], vec![100, 102, 104]);
    assert_eq!(shown["mcp"], vec![100, 102, 104]);
    assert_eq!(
        shown["telemetry"],
        (0..10).map(|n| 100 + 2 * n).collect::<Vec<i64>>()
    );
}

/// A telemetria csak nyitott döntésnél jelenik meg (nem döntött / „most nem”); Never / Off után a hívó
/// `telemetry_open: false`-t ad, és akkor sem, ha a számláló még nem fogyott el.
#[test]
fn telemetry_is_offered_only_while_the_decision_is_open() {
    let mut closed = facts();
    closed.telemetry_open = false;
    let (items, _) = decide(&State::default(), &closed, 100);
    assert_eq!(items, vec![Item::Hook, Item::Mcp]);
    let mut state = State::default();
    state.never(Item::Telemetry);
    assert!(!decide(&state, &facts(), 100).0.contains(&Item::Telemetry));
}

#[test]
fn telemetry_text_explains_the_benefit_and_links_the_preview() {
    let out = render(&[Item::Telemetry], &facts(), &commands()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let message = v["systemMessage"].as_str().unwrap();
    assert!(message.contains('⭐'), "{message}");
    assert!(
        message.contains("bash /s/callgraph.sh telemetry preview"),
        "{message}"
    );
    assert!(
        message.contains("bash /s/callgraph.sh telemetry on"),
        "{message}"
    );
    let lang = crate::i18n::lang();
    assert!(
        message.contains(lang.pick("locate ranking", "locate rangsorát")),
        "{message}"
    );
}

#[test]
fn never_silences_only_that_item() {
    let mut state = State::default();
    state.never(Item::Telemetry);
    let (items, _) = decide(&state, &facts(), 100);
    assert_eq!(items, vec![Item::Hook, Item::Mcp]);
}

#[test]
fn reset_brings_an_item_back() {
    let mut state = State::default();
    state.never(Item::Hook);
    assert!(!decide(&state, &facts(), 100).0.contains(&Item::Hook));
    state.reset(Some(Item::Hook));
    assert!(decide(&state, &facts(), 100).0.contains(&Item::Hook));
    state.never(Item::Mcp);
    state.never(Item::Telemetry);
    state.reset(None);
    assert_eq!(
        decide(&state, &facts(), 100).0,
        vec![Item::Hook, Item::Mcp, Item::Telemetry]
    );
}

#[test]
fn only_missing_things_are_recommended() {
    let mut f = all_good();
    f.mcp = McpState::WithoutAutoUpdate;
    assert_eq!(decide(&State::default(), &f, 1).0, vec![Item::Mcp]);
    f.mcp = McpState::Unknown;
    assert!(decide(&State::default(), &f, 1).0.is_empty());
}

#[test]
fn update_notice_is_shown_without_items_and_without_state() {
    let mut f = all_good();
    f.update = Some("0.9.5".into());
    let (items, next) = decide(&State::default(), &f, 1);
    assert!(items.is_empty());
    assert_eq!(next, State::default());
    let out = render(&items, &f, &commands()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v["systemMessage"].as_str().unwrap().contains("0.9.5"));
    assert!(
        v["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap()
            .contains("bash /s/callgraph.sh update")
    );
    assert_eq!(v["hookSpecificOutput"]["hookEventName"], "SessionStart");
}

#[test]
fn agent_is_told_never_to_enable_telemetry_itself() {
    let items = vec![Item::Telemetry];
    let out = render(&items, &facts(), &commands()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let context = v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("do NOT turn it on yourself"));
    assert!(context.contains("in their own terminal"));
    // a bekapcsoló parancs csak említésként szerepel, a „ne” parancs futtatható
    assert!(context.contains("hook never telemetry"));
    let message = v["systemMessage"].as_str().unwrap();
    assert!(message.contains("bash /s/callgraph.sh telemetry on"));
}

#[test]
fn every_change_requires_an_explicit_yes() {
    let out = render(&[Item::Hook, Item::Mcp], &facts(), &commands()).unwrap();
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let context = v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap();
    assert!(context.contains("act only on an explicit yes"));
    assert!(context.contains("hook install"));
    assert!(context.contains("mcp --auto-update"));
    assert!(context.contains("hook never hook"));
    assert!(context.contains("hook never mcp"));
}

#[test]
fn disabled_by_switch_or_ci() {
    let env = |pairs: &'static [(&'static str, &'static str)]| {
        move |k: &str| {
            pairs
                .iter()
                .find(|(n, _)| *n == k)
                .map(|(_, v)| (*v).to_owned())
        }
    };
    assert!(!disabled(env(&[])));
    assert!(disabled(env(&[("CALLGRAPH_RECOMMEND", "0")])));
    assert!(disabled(env(&[("CALLGRAPH_RECOMMEND", "off")])));
    assert!(disabled(env(&[("CI", "true")])));
    assert!(!disabled(env(&[("CALLGRAPH_RECOMMEND", "1"), ("CI", "")])));
}

#[test]
fn mcp_state_from_claude_json() {
    assert_eq!(mcp_state(None, false), McpState::Unknown);
    assert_eq!(mcp_state(None, true), McpState::Missing);
    assert_eq!(mcp_state(Some("{}"), true), McpState::Missing);
    assert_eq!(mcp_state(Some("not json"), true), McpState::Unknown);
    let plain = r#"{"mcpServers":{"callgraph":{"command":"/x/callgraph","args":["mcp"]}}}"#;
    assert_eq!(mcp_state(Some(plain), true), McpState::WithoutAutoUpdate);
    let auto =
        r#"{"mcpServers":{"callgraph":{"command":"/x/callgraph","args":["mcp","--auto-update"]}}}"#;
    assert_eq!(mcp_state(Some(auto), true), McpState::Ok);
}

#[test]
fn item_keys_round_trip() {
    for item in Item::ALL {
        assert_eq!(Item::parse(item.key()), Some(item));
    }
    assert_eq!(Item::parse("other"), None);
}
