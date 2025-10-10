use metrics::{counter, gauge, Counter, Gauge};
use std::sync::LazyLock;

/// Metric name for counting router calls.
pub(crate) const ROUTER_CALLS: &str = "ajj.router.calls";
pub(crate) const ROUTER_CALLS_HELP: &str =
    "Number of calls to ajj router methods. Not all requests will result in a response.";

/// Metric name for counting router error execution.
pub(crate) const ROUTER_ERRORS: &str = "ajj.router.errors";
pub(crate) const ROUTER_ERRORS_HELP: &str =
    "Number of errored executions by ajj router methods. This does NOT imply a response was sent.";

// Metric name for counting router successful executions.
pub(crate) const ROUTER_SUCCESSES: &str = "ajj.router.successes";
pub(crate) const ROUTER_SUCCESSES_HELP: &str =
    "Number of successful executions by ajj router methods. This does NOT imply a response was sent.";

/// Metric name for counting router responses.
pub(crate) const ROUTER_RESPONSES: &str = "ajj.router.responses";
pub(crate) const ROUTER_RESPONSES_HELP: &str =
    "Number of responses sent by ajj router methods. Not all requests will result in a response.";

// Metric name for counting omitted notification responses.
pub(crate) const ROUTER_NOTIFICATION_RESPONSE_OMITTED: &str =
    "ajj.router.notification_response_omitted";
pub(crate) const ROUTER_NOTIFICATION_RESPONSE_OMITTED_HELP: &str =
    "Number of times ajj router methods omitted a response to a notification";

// Metric for counting parse errors.
pub(crate) const ROUTER_PARSE_ERRORS: &str = "ajj.router.parse_errors";
pub(crate) const ROUTER_PARSE_ERRORS_HELP: &str =
    "Number of parse errors encountered by ajj router methods. This implies no response was sent.";

/// Metric for counting method not found errors.
pub(crate) const ROUTER_METHOD_NOT_FOUND: &str = "ajj.router.method_not_found";
pub(crate) const ROUTER_METHOD_NOT_FOUND_HELP: &str =
    "Number of times ajj router methods encountered a method not found error. This implies a response was sent.";

/// Metric for tracking active calls.
pub(crate) const ACTIVE_CALLS: &str = "ajj.router.active_calls";
pub(crate) const ACTIVE_CALLS_HELP: &str = "Number of active calls being processed";

/// Metric for tracking completed   calls.
pub(crate) const COMPLETED_CALLS: &str = "ajj.router.completed_calls";
pub(crate) const COMPLETED_CALLS_HELP: &str = "Number of completed calls handled";

static DESCRIBE: LazyLock<()> = LazyLock::new(|| {
    metrics::describe_counter!(ROUTER_CALLS, metrics::Unit::Count, ROUTER_CALLS_HELP);
    metrics::describe_counter!(ROUTER_ERRORS, metrics::Unit::Count, ROUTER_ERRORS_HELP);
    metrics::describe_counter!(
        ROUTER_SUCCESSES,
        metrics::Unit::Count,
        ROUTER_SUCCESSES_HELP
    );
    metrics::describe_counter!(
        ROUTER_RESPONSES,
        metrics::Unit::Count,
        ROUTER_RESPONSES_HELP
    );
    metrics::describe_counter!(
        ROUTER_NOTIFICATION_RESPONSE_OMITTED,
        metrics::Unit::Count,
        ROUTER_NOTIFICATION_RESPONSE_OMITTED_HELP
    );
    metrics::describe_counter!(
        ROUTER_PARSE_ERRORS,
        metrics::Unit::Count,
        ROUTER_PARSE_ERRORS_HELP
    );
    metrics::describe_counter!(
        ROUTER_METHOD_NOT_FOUND,
        metrics::Unit::Count,
        ROUTER_METHOD_NOT_FOUND_HELP
    );
    metrics::describe_gauge!(ACTIVE_CALLS, metrics::Unit::Count, ACTIVE_CALLS_HELP);
    metrics::describe_counter!(COMPLETED_CALLS, metrics::Unit::Count, COMPLETED_CALLS_HELP);
});

/// Get or register a counter for calls to a specific service and method.
pub(crate) fn calls(service_name: &'static str, method: &str) -> Counter {
    let _ = &DESCRIBE;
    counter!(
        ROUTER_CALLS,
        "service" => service_name.to_string(),
        "method" => method.to_string()
    )
}

/// Record a call to a specific service and method.
pub(crate) fn record_call(service_name: &'static str, method: &str) {
    let counter = calls(service_name, method);
    counter.increment(1);
    increment_active_calls(service_name, method);
}

/// Get or register a counter for errors from a specific service and method.
pub(crate) fn errors(service_name: &'static str, method: &str) -> Counter {
    let _ = &DESCRIBE;
    counter!(
        ROUTER_ERRORS,
        "service" => service_name.to_string(),
        "method" => method.to_string()
    )
}

/// Record an error from a specific service and method.
pub(crate) fn record_execution_error(service_name: &'static str, method: &str) {
    let counter = errors(service_name, method);
    counter.increment(1);
}

/// Get or register a counter for successes from a specific service and method.
pub(crate) fn successes(service_name: &'static str, method: &str) -> Counter {
    let _ = &DESCRIBE;
    counter!(
        ROUTER_SUCCESSES,
        "service" => service_name.to_string(),
        "method" => method.to_string()
    )
}

/// Record a success from a specific service and method.
pub(crate) fn record_execution_success(service_name: &'static str, method: &str) {
    let counter = successes(service_name, method);
    counter.increment(1);
}

/// Record a response from a specific service and method, incrementing either
/// the success or error counter.
pub(crate) fn record_execution(success: bool, service_name: &'static str, method: &str) {
    if success {
        record_execution_success(service_name, method);
    } else {
        record_execution_error(service_name, method);
    }
}

/// Get or register a counter for responses from a specific service and method.
pub(crate) fn responses(service_name: &'static str, method: &str) -> Counter {
    let _ = &DESCRIBE;
    counter!(
        ROUTER_RESPONSES,
        "service" => service_name.to_string(),
        "method" => method.to_string()
    )
}

/// Record a response from a specific service and method.
pub(crate) fn record_response(service_name: &'static str, method: &str) {
    let counter = responses(service_name, method);
    counter.increment(1);
}

/// Get or register a counter for omitted notification responses from a specific service and method.
pub(crate) fn response_omitted(service_name: &'static str, method: &str) -> Counter {
    let _ = &DESCRIBE;
    counter!(
        ROUTER_NOTIFICATION_RESPONSE_OMITTED,
        "service" => service_name.to_string(),
        "method" => method.to_string()
    )
}

/// Record an omitted notification response from a specific service and method.
pub(crate) fn record_response_omitted(service_name: &'static str, method: &str) {
    let counter = response_omitted(service_name, method);
    counter.increment(1);
}

/// Record either a response sent or an omitted notification response.
pub(crate) fn record_output(response_sent: bool, service_name: &'static str, method: &str) {
    if response_sent {
        record_response(service_name, method);
    } else {
        record_response_omitted(service_name, method);
    }
    record_completed_call(service_name, method);
    decrement_active_calls(service_name, method);
}

// Get or register a counter for parse errors.
pub(crate) fn parse_errors(service_name: &'static str) -> Counter {
    let _ = &DESCRIBE;
    counter!(ROUTER_PARSE_ERRORS, "service" => service_name.to_string())
}

/// Record a parse error.
pub(crate) fn record_parse_error(service_name: &'static str) {
    let counter = parse_errors(service_name);
    counter.increment(1);
}

/// Get or register a counter for method not found errors.
pub(crate) fn method_not_found_errors(service_name: &'static str, method: &str) -> Counter {
    let _ = &DESCRIBE;
    counter!(ROUTER_METHOD_NOT_FOUND, "service" => service_name.to_string(), "method" => method.to_string())
}

/// Record a method not found error.
pub(crate) fn record_method_not_found(
    response_sent: bool,
    service_name: &'static str,
    method: &str,
) {
    let counter = method_not_found_errors(service_name, method);
    counter.increment(1);
    record_output(response_sent, service_name, method);
}

/// Get or register a gauge for active calls to a specific service.
pub(crate) fn active_calls(service_name: &'static str, method: &str) -> Gauge {
    let _ = &DESCRIBE;
    gauge!(ACTIVE_CALLS, "service" => service_name.to_string(), "method" => method.to_string())
}

/// Increment the active calls gauge for a specific service.
pub(crate) fn increment_active_calls(service_name: &'static str, method: &str) {
    let _ = &DESCRIBE;
    let gauge = active_calls(service_name, method);
    gauge.increment(1);
}

/// Decrement the active calls gauge for a specific service.
pub(crate) fn decrement_active_calls(service_name: &'static str, method: &str) {
    let _ = &DESCRIBE;
    let gauge = active_calls(service_name, method);
    gauge.decrement(1);
}

/// Get or register a counter for completed calls to a specific service.
pub(crate) fn completed_calls(service_name: &'static str, method: &str) -> Counter {
    let _ = &DESCRIBE;
    counter!(
        COMPLETED_CALLS,
        "service" => service_name.to_string(),
        "method" => method.to_string()
    )
}

/// Record a completed call to a specific service and method.
pub(crate) fn record_completed_call(service_name: &'static str, method: &str) {
    let _ = &DESCRIBE;
    let counter = completed_calls(service_name, method);
    counter.increment(1);
}
