//! [`serde_with`] helpers.

use std::fmt;
use std::marker::PhantomData;

use serde::de::{Deserializer, Error as DeError, IgnoredAny, MapAccess, Visitor};
use serde_with::de::{DeserializeAs, DeserializeAsWrap};
use serde_with::SerializeAs;

/// Deserialize a tuple sequence from a map, ignoring keys.
pub struct IgnoreKeys<T>(PhantomData<T>);

macro_rules! tuple_impl {
    ($len:literal $($n:tt $t:ident $tas:ident)+) => {
        impl<'de, $($t, $tas,)+> DeserializeAs<'de, ($($t,)+)> for IgnoreKeys<($($tas,)+)>
        where
            $($tas: DeserializeAs<'de, $t>,)+
        {
            fn deserialize_as<D>(deserializer: D) -> Result<($($t,)+), D::Error>
            where
                D: Deserializer<'de>,
            {
                struct MapVisitor<$($t,)+>(PhantomData<($($t,)+)>);

                impl<'de, $($t, $tas,)+> Visitor<'de>
                    for MapVisitor<$(DeserializeAsWrap<$t, $tas>,)+>
                where
                    $($tas: DeserializeAs<'de, $t>,)+
                {
                    type Value = ($($t,)+);

                    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                        formatter.write_str(concat!("a map of size ", $len))
                    }

                    #[allow(non_snake_case)]
                    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
                    where
                        A: MapAccess<'de>,
                    {
                        $(
                            let $t: (IgnoredAny, DeserializeAsWrap<$t, $tas>) = match map.next_entry()? {
                                Some(value) => value,
                                None => return Err(DeError::invalid_length($n, &self)),
                            };
                        )+

                        Ok(($($t.1.into_inner(),)+))
                    }
                }

                deserializer.deserialize_map(
                    MapVisitor::<$(DeserializeAsWrap<$t, $tas>,)+>(PhantomData),
                )
            }
        }
    };
}

tuple_impl!(1 0 T0 As0);
tuple_impl!(2 0 T0 As0 1 T1 As1);
tuple_impl!(3 0 T0 As0 1 T1 As1 2 T2 As2);
tuple_impl!(4 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3);
tuple_impl!(5 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4);
tuple_impl!(6 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4 5 T5 As5);
tuple_impl!(7 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4 5 T5 As5 6 T6 As6);
tuple_impl!(8 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4 5 T5 As5 6 T6 As6 7 T7 As7);
tuple_impl!(9 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4 5 T5 As5 6 T6 As6 7 T7 As7 8 T8 As8);
tuple_impl!(10 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4 5 T5 As5 6 T6 As6 7 T7 As7 8 T8 As8 9 T9 As9);
tuple_impl!(11 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4 5 T5 As5 6 T6 As6 7 T7 As7 8 T8 As8 9 T9 As9 10 T10 As10);
tuple_impl!(12 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4 5 T5 As5 6 T6 As6 7 T7 As7 8 T8 As8 9 T9 As9 10 T10 As10 11 T11 As11);
tuple_impl!(13 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4 5 T5 As5 6 T6 As6 7 T7 As7 8 T8 As8 9 T9 As9 10 T10 As10 11 T11 As11 12 T12 As12);
tuple_impl!(14 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4 5 T5 As5 6 T6 As6 7 T7 As7 8 T8 As8 9 T9 As9 10 T10 As10 11 T11 As11 12 T12 As12 13 T13 As13);
tuple_impl!(15 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4 5 T5 As5 6 T6 As6 7 T7 As7 8 T8 As8 9 T9 As9 10 T10 As10 11 T11 As11 12 T12 As12 13 T13 As13 14 T14 As14);
tuple_impl!(16 0 T0 As0 1 T1 As1 2 T2 As2 3 T3 As3 4 T4 As4 5 T5 As5 6 T6 As6 7 T7 As7 8 T8 As8 9 T9 As9 10 T10 As10 11 T11 As11 12 T12 As12 13 T13 As13 14 T14 As14 15 T15 As15);

/// `serde_with` to convert from [`web_time::SystemTime`] to [`std::time::SystemTime`].
pub struct WebSystemTime<T>(PhantomData<T>);
impl<'de, T> DeserializeAs<'de, web_time::SystemTime> for WebSystemTime<T>
where
    T: DeserializeAs<'de, std::time::SystemTime>,
{
    fn deserialize_as<D>(deserializer: D) -> Result<web_time::SystemTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        let time = T::deserialize_as(deserializer)?;
        Ok(<web_time::SystemTime as web_time::web::SystemTimeExt>::from_std(time))
    }
}
impl<T> SerializeAs<web_time::SystemTime> for WebSystemTime<T>
where
    T: SerializeAs<std::time::SystemTime>,
{
    fn serialize_as<S>(source: &web_time::SystemTime, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        T::serialize_as(
            &<web_time::SystemTime as web_time::web::SystemTimeExt>::to_std(*source),
            serializer,
        )
    }
}
