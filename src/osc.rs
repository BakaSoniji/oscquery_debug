use rosc::{OscMessage, OscType};

pub fn parse_vrsystem_osc(msg: &OscMessage) -> Option<(&str, [f32; 3], [f32; 3])> {
    let path = &msg.addr;
    let part = path
        .strip_prefix("/tracking/vrsystem/")?
        .strip_suffix("/pose")?;

    match part {
        "head" | "leftwrist" | "rightwrist" => {}
        _ => return None,
    }

    if msg.args.len() < 6 {
        return None;
    }

    let floats: Vec<f32> = msg
        .args
        .iter()
        .take(6)
        .filter_map(|a| match a {
            OscType::Float(f) => Some(*f),
            OscType::Double(d) => Some(*d as f32),
            _ => None,
        })
        .collect();

    if floats.len() < 6 {
        return None;
    }

    Some((
        part,
        [floats[0], floats[1], floats[2]],
        [floats[3], floats[4], floats[5]],
    ))
}

pub fn format_osc_args(args: &[OscType]) -> String {
    let parts: Vec<String> = args
        .iter()
        .map(|a| match a {
            OscType::Float(f) => format!("{f}"),
            OscType::Double(d) => format!("{d}"),
            OscType::Int(i) => format!("{i}"),
            OscType::Long(l) => format!("{l}"),
            OscType::String(s) => format!("\"{s}\""),
            OscType::Bool(b) => format!("{b}"),
            OscType::Nil => "nil".to_string(),
            other => format!("{other:?}"),
        })
        .collect();
    format!("[{}]", parts.join(", "))
}
