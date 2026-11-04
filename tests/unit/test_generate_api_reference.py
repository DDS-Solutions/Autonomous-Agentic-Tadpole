"""
@docs ARCHITECTURE:TestSuites

### AI Assist Note
Verifies Rust router scanning across nested calls, escaped literals, and comments.

### 🔍 Debugging & Observability
- **Failure Path**: Route discovery skips valid routes or accepts comment/string examples.
- **Telemetry Link**: Search [generate_api_reference] in Python test output.
"""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "execution"))
import generate_api_reference


class TestGenerateApiReferenceScanner(unittest.TestCase):
    def test_extracts_nested_route_calls_and_ignores_comments(self):
        source = '''
fn sample_routes() -> Router {
    Router::new()
        // .route("/line-comment", get(fake_handler()))
        /* Nested comment with braces { /* and .route("/nested-comment", fake()) */ } */
        .route("/nested", get(handler.with_state(make_state(vec![1, 2]))))
        .route("/escaped", get(handler_with("quote: \\\" and ) ,", nested())))
}
'''

        body = generate_api_reference.extract_function_body(source, "sample_routes")
        routes = list(generate_api_reference.extract_route_calls(body))

        self.assertEqual([route[0] for route in routes], ["/nested", "/escaped"])
        self.assertEqual(routes[0][1], "get(handler.with_state(make_state(vec![1, 2])))")
        self.assertIn('quote: \\\" and ) ,', routes[1][1])

    def test_extract_function_body_ignores_braces_in_literals_and_comments(self):
        source = r'''fn sample() {
    let text = "a } brace and escaped quote: \"";
    /* outer { /* nested } */ still comment } */
    .route("/ok", get(handler()))
}'''

        body = generate_api_reference.extract_function_body(source, "sample")

        self.assertIn('.route("/ok", get(handler()))', body)
        self.assertNotIn("still comment } */\n}", body)

    def test_discover_routes_keeps_current_routes_and_total_stable(self):
        routes = generate_api_reference.discover_routes()
        route_keys = {(route.method, route.path, route.handler) for route in routes}

        self.assertEqual(len(routes), 143)
        self.assertIn(("GET", "/v1/agents/{id}/context-packet", "routes::agentic_engine::get_context_packet"), route_keys)
        self.assertIn(("POST", "/v1/engine/deploy", "routes::deploy::trigger_deploy"), route_keys)


if __name__ == "__main__":
    unittest.main()
