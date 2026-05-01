mod bindings {
    wit_bindgen::generate!({
        world: "shapes-svc",
        generate_all
    });
}

use crate::bindings::exports::my::service::shapes::{
    AccessPerms, Color, Event, Guest, Person, Priority,
};

pub struct Service;

impl Guest for Service {
    fn pick_color(p: Priority) -> Color {
        // println!("     [shapes] pick-color");
        match p {
            Priority::Low => Color::Green,
            Priority::Medium => Color::Blue,
            Priority::High => Color::Red,
        }
    }

    fn check_perms(p: AccessPerms) -> bool {
        // println!("     [shapes] check-perms");
        p.contains(AccessPerms::READ) && p.contains(AccessPerms::WRITE)
    }

    fn greet(p: Person) -> String {
        // println!("     [shapes] greet");
        format!("hello, {} (age {})!", p.name, p.age)
    }

    fn describe_event(e: Event) -> String {
        // println!("     [shapes] describe-event");
        match e {
            Event::Click(coord) => format!("click({coord})"),
            Event::Keypress(k) => format!("keypress({k})"),
            Event::Idle => "idle".to_string(),
        }
    }

    fn swap_pair(pair: (i32, String)) -> (String, i32) {
        (pair.1, pair.0)
    }

    fn maybe_double(x: Option<i32>) -> Option<i32> {
        x.map(|v| v.saturating_mul(2))
    }

    fn divide(a: i32, b: i32) -> Result<i32, String> {
        if b == 0 {
            Err("divide by zero".to_string())
        } else {
            Ok(a / b)
        }
    }

    fn sum(xs: Vec<i32>) -> i32 {
        xs.iter().copied().sum()
    }

    fn to_string(c: char) -> String {
        c.to_string()
    }

    fn aggregate(p: Person, scale: Option<u32>) -> Result<Vec<i32>, String> {
        // println!("     [shapes] aggregate");
        let s = scale.unwrap_or(1) as i32;
        Ok(vec![(p.age as i32).saturating_mul(s), (p.name.len() as i32).saturating_mul(s)])
    }
}

bindings::export!( Service with_types_in bindings );
