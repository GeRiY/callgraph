use crate::lang::php::categorize::{CallInfo, CategoryChain};
use crate::lang::php::framework::{GenericStrategy, LaravelStrategy};

// tesztsegéd: a CallInfo összes mezője pozíciósan, a táblázatos esetekhez
#[allow(clippy::too_many_arguments)]
fn call<'a>(
    target: &'a str,
    kind: &'a str,
    resolved: Option<&'a str>,
    short: Option<&'a str>,
    on: Option<&'a str>,
    imported: bool,
    this: bool,
    framework: &'a dyn crate::lang::php::framework::FrameworkStrategy,
) -> CallInfo<'a> {
    CallInfo {
        kind,
        target,
        on,
        resolved,
        class_short_name: short,
        has_use_import: imported,
        is_this: this,
        framework,
    }
}

#[test]
fn representative_rules_cover_the_category_chain() {
    let generic = &GenericStrategy;
    let laravel = &LaravelStrategy;
    let cases = [
        (
            call("time", "func_call", None, None, None, false, false, generic),
            "native_php",
        ),
        (
            call("__", "func_call", None, None, None, false, false, generic),
            "global_helper",
        ),
        (
            call(
                "asJson",
                "method_call",
                None,
                None,
                Some("this"),
                false,
                true,
                generic,
            ),
            "internal_helper",
        ),
        (
            call(
                "belongsTo",
                "method_call",
                None,
                None,
                Some("this"),
                false,
                true,
                generic,
            ),
            "orm_relation",
        ),
        (
            call(
                "Log::debug",
                "static_call",
                Some("Illuminate\\Support\\Facades\\Log::debug"),
                Some("Log"),
                None,
                false,
                false,
                laravel,
            ),
            "framework_use",
        ),
        (
            call(
                "ApiAccessKey::find",
                "static_call",
                Some("app\\models\\ApiAccessKey::find"),
                Some("ApiAccessKey"),
                None,
                true,
                false,
                generic,
            ),
            "model_use",
        ),
        (
            call(
                "entry\\components\\ApplySummary",
                "new",
                Some("entry\\components\\ApplySummary"),
                Some("ApplySummary"),
                None,
                true,
                false,
                generic,
            ),
            "component_use",
        ),
        (
            call(
                "ext\\PaymentGateway::getApi",
                "static_call",
                Some("ext\\PaymentGateway::getApi"),
                Some("PaymentGateway"),
                None,
                true,
                false,
                generic,
            ),
            "static_call",
        ),
        (
            call(
                "one",
                "method_call",
                None,
                None,
                Some("<expr>"),
                false,
                false,
                generic,
            ),
            "external_call",
        ),
    ];
    for (input, expected) in cases {
        assert_eq!(
            CategoryChain::with_defaults().categorize(&input),
            expected,
            "{}",
            input.target
        );
    }
}
