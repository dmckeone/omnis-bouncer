pub fn to_redis_bool(val: bool) -> isize {
    match val {
        true => 1,
        false => 0,
    }
}

pub fn from_redis_bool(val: isize, default: bool) -> bool {
    match val {
        1 => true,
        0 => false,
        _ => default,
    }
}

mod test {
    use super::*;

    #[test]
    fn test_to_redis_bool_one() {
        assert_eq!(to_redis_bool(true), 1);
    }

    #[test]
    fn test_to_redis_bool_zero() {
        assert_eq!(to_redis_bool(false), 0);
    }

    #[test]
    fn test_from_redis_bool_one() {
        assert_eq!(from_redis_bool(1, false), true);
    }

    #[test]
    fn test_from_redis_bool_zero() {
        assert_eq!(from_redis_bool(0, true), false);
    }

    #[test]
    fn test_from_redis_bool_default() {
        assert_eq!(from_redis_bool(2, true), true);
        assert_eq!(from_redis_bool(2, false), false);
    }
}
