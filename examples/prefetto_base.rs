use std::thread;
use std::time::Duration;

use tracing::{debug_span, event, Level};
use tracing_profile::init_tracing;

fn make_spans() {
    event!(name: "event outside of span", Level::DEBUG, {value = 10});
    event!(name: "test_instant_event", Level::DEBUG, test_key = "test_value");

    {
        let span = debug_span!("root span");
        let _scope1 = span.enter();
        thread::sleep(Duration::from_millis(20));

        // child spans 1 and 2 are siblings
        let span2 = debug_span!("child span1", field1 = "value1", perfetto_track_id = 5);
        let scope2 = span2.enter();
        thread::sleep(Duration::from_millis(20));
        drop(scope2);

        let span3 = debug_span!(
            "child span2",
            field2 = "value2",
            value = 20,
            perfetto_track_id = 5,
            perfetto_flow_id = 10
        );
        let _scope3 = span3.enter();

        thread::sleep(Duration::from_millis(20));
        event!(name: "event in span2", Level::DEBUG, {value = 100});

        // child spans 3 and 4 are siblings
        let span = debug_span!("child span3", field3 = "value3");
        let scope = span.enter();
        thread::sleep(Duration::from_millis(20));
        event!(name: "custom event", Level::DEBUG, {field5 = "value5", counter = true, value = 30});
        drop(scope);

        thread::spawn(|| {
            let span = debug_span!("child span5", field5 = "value5");
            let _scope = span.enter();
            thread::sleep(Duration::from_millis(20));
            event!(name: "custom event", Level::DEBUG, {field5 = "value6", counter = true, value = 10});
        }).join().unwrap();

        let span = debug_span!("child span4", field4 = "value4", perfetto_flow_id = 10);
        thread::sleep(Duration::from_millis(20));
        event!(name: "custom event", Level::DEBUG, {field5 = "value5", counter = true, value = 40});
        let scope = span.enter();
        thread::sleep(Duration::from_millis(20));
        drop(scope);
    }
    event!(name: "event after last span", Level::DEBUG, {value = 20});
}

fn main() {
    // let _test_dir = PerfettoTestDir::new();
    let _guard = init_tracing().unwrap();

    make_spans();
}
