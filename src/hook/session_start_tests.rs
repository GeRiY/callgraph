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
    for day in 100..120 {
        let (items, next) = decide(&state, &facts(), day);
        if !items.is_empty() {
            assert_eq!(items, vec![Item::Hook, Item::Mcp, Item::Telemetry]);
            shown_days.push(day);
        }
        state = next;
    }
    assert_eq!(shown_days, vec![100, 102, 104]);
}

#[test]
fn never_silences_only_that_item() {
    let mut state = State::default();
    state.never(Item::Telemetry);
    let (items, _) = decide(&state, &facts(), 100);
    assert_eq!(items, vec![Item::Hook, Item::Mcp]);
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
