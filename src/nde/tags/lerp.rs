pub(super) trait Lerp<U = Self> {
    type Output;

    fn lerp(self, other: U, power: f64) -> Self::Output;
    fn out(self) -> Self::Output;
}

impl Lerp for i32 {
    type Output = i32;

    #[allow(clippy::cast_possible_truncation)]
    fn lerp(self, other: Self, power: f64) -> Self::Output {
        f64::from(self).mul_add(1.0 - power, f64::from(other) * power) as i32
    }

    fn out(self) -> Self::Output {
        self
    }
}

impl Lerp for u32 {
    type Output = u32;

    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    fn lerp(self, other: Self, power: f64) -> Self::Output {
        f64::from(self).mul_add(1.0 - power, f64::from(other) * power) as u32
    }

    fn out(self) -> Self::Output {
        self
    }
}

impl Lerp for u8 {
    type Output = u8;

    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    fn lerp(self, other: Self, power: f64) -> Self::Output {
        f64::from(self).mul_add(1.0 - power, f64::from(other) * power) as u8
    }

    fn out(self) -> Self::Output {
        self
    }
}

impl Lerp for f64 {
    type Output = f64;

    fn lerp(self, other: Self, power: f64) -> Self::Output {
        self.mul_add(1.0 - power, other * power)
    }

    fn out(self) -> Self::Output {
        self
    }
}

impl<T> Lerp for Option<T>
where
    T: Lerp,
{
    type Output = Option<T::Output>;

    fn lerp(self, other: Self, power: f64) -> Self::Output {
        match self {
            None => other.out(),
            Some(val1) => match other {
                None => Some(val1.out()),
                Some(val2) => Some(val1.lerp(val2, power)),
            },
        }
    }

    fn out(self) -> Self::Output {
        self.map(T::out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn various_types() {
        assert_eq!((-3_i32).lerp(3_i32, 0.5), 0_i32);
        assert_eq!((3_u32).lerp(6_u32, 0.334), 4_u32);
        assert_eq!((3_u8).lerp(6_u8, 0.334), 4_u8);
        assert!((3.0_f64.lerp(4.0_f64, 0.25) - 3.25_f64).abs() < f64::EPSILON);

        assert_eq!(None::<i32>.lerp(None, 0.5), None);
        assert_eq!(None.lerp(Some(3_i32), 0.5), Some(3_i32));
        assert_eq!(Some(-3_i32).lerp(None, 0.5), Some(-3_i32));
        assert_eq!(Some(-3_i32).lerp(Some(3_i32), 0.5), Some(0_i32));
    }
}
