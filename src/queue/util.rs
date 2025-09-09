pub(crate) fn to_redis_bool(val: bool) -> isize {
    match val {
        true => 1,
        false => 0,
    }
}

pub(crate) fn from_redis_bool(val: isize, default: bool) -> bool {
    match val {
        1 => true,
        0 => false,
        _ => default,
    }
}

mod test {}
