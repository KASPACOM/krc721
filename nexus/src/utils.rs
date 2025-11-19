use crate::imports::*;

pub fn get_home_dir() -> PathBuf {
    cfg_if! {
        if #[cfg(target_os = "windows")] {
            dirs::data_local_dir().unwrap()
        } else {
            dirs::home_dir().unwrap()
        }
    }
}

/// Get the default application directory.
pub fn get_app_dir() -> PathBuf {
    cfg_if! {
        if #[cfg(target_os = "windows")] {
            get_home_dir().join("krc721")
        } else {
            get_home_dir().join(".krc721")
        }
    }
}
use std::iter::FromIterator;

/// A container for an initialization sequence and an optional last element.
#[derive(Debug, PartialEq)]
pub struct SplitLast<B, T> {
    init: B,
    last: Option<T>,
}

/// Collects an iterator into a tuple of initialization sequence and last element.
///
/// # Type Parameters
///
/// * `C`: Collection type for storing initialization sequence
/// * `T`: Element type
///
/// # Examples
///
/// ```
/// use std::collections::VecDeque;
/// # use krc721_nexus::utils::SplitLastCollector;
///
/// let vec = vec![1, 2, 3, 4];
/// let result: SplitLastCollector<Vec<_>, _> = vec.into_iter().collect();
/// assert_eq!(result.into_inner(), Some((vec![1, 2, 3], 4)));
///
/// // Works with different collection types
/// let result: SplitLastCollector<VecDeque<_>, _> = vec![1, 2, 3].into_iter().collect();
/// let mut expected = VecDeque::new();
/// expected.extend([1, 2]);
/// assert_eq!(result.into_inner(), Some((expected, 3)));
/// ```
#[derive(Debug, PartialEq)]
pub struct SplitLastCollector<C, T>(pub Option<(C, T)>);

impl<C, T> From<SplitLastCollector<C, T>> for Option<(C, T)> {
    fn from(value: SplitLastCollector<C, T>) -> Self {
        value.0
    }
}

impl<C, T> From<Option<(C, T)>> for SplitLastCollector<C, T> {
    fn from(value: Option<(C, T)>) -> Self {
        Self(value)
    }
}

impl<C, T> SplitLastCollector<C, T> {
    /// Extracts the inner value from the collector.
    pub fn into_inner(self) -> Option<(C, T)> {
        self.0
    }
}

impl<T, C> FromIterator<T> for SplitLastCollector<C, T>
where
    C: Default + Extend<T>,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut iter = iter.into_iter();

        let first = match iter.next() {
            Some(first) => first,
            None => return SplitLastCollector(None),
        };

        match iter.next() {
            None => SplitLastCollector(Some((C::default(), first))),
            Some(second) => {
                let mut init = C::default();
                init.extend(std::iter::once(first));

                let mut last = second;
                for item in iter {
                    init.extend(std::iter::once(last));
                    last = item;
                }

                SplitLastCollector(Some((init, last)))
            }
        }
    }
}

/// Collects an iterator of `Result`s into a tuple of initialization sequence and last element.
/// Short-circuits on first error encountered.
///
/// # Type Parameters
///
/// * `C`: Collection type for storing initialization sequence
/// * `T`: Success type of the `Result`
/// * `E`: Error type of the `Result`
///
/// # Examples
///
/// ```
/// # use krc721_nexus::utils::ResultSplitLastCollector;
/// let vec: Vec<Result<i32, &str>> = vec![Ok(1), Ok(2), Ok(3)];
/// let result: ResultSplitLastCollector<Vec<_>, _, _> = vec.into_iter().collect();
/// assert_eq!(result.into_inner(), Ok(Some((vec![1, 2], 3))));
///
/// // Short-circuits on error
/// let vec = vec![Ok(1), Err("error"), Ok(3)];
/// let result: ResultSplitLastCollector<Vec<_>, _, _> = vec.into_iter().collect();
/// assert_eq!(result.into_inner(), Err("error"));
/// ```
#[derive(Debug, PartialEq)]
pub struct ResultSplitLastCollector<C, T, E>(pub Result<Option<(C, T)>, E>);

impl<C, T, E> From<ResultSplitLastCollector<C, T, E>> for Result<Option<(C, T)>, E> {
    fn from(value: ResultSplitLastCollector<C, T, E>) -> Self {
        value.0
    }
}

impl<C, T, E> From<Result<Option<(C, T)>, E>> for ResultSplitLastCollector<C, T, E> {
    fn from(value: Result<Option<(C, T)>, E>) -> Self {
        Self(value)
    }
}

impl<C, T, E> ResultSplitLastCollector<C, T, E> {
    /// Extracts the inner value from the collector.
    pub fn into_inner(self) -> Result<Option<(C, T)>, E> {
        self.0
    }
}

impl<T, E, C> FromIterator<Result<T, E>> for ResultSplitLastCollector<C, T, E>
where
    C: Default + Extend<T>,
{
    fn from_iter<I: IntoIterator<Item = Result<T, E>>>(iter: I) -> Self {
        let mut init = C::default();
        let mut last = None;

        for item in iter {
            match item {
                Ok(value) => {
                    if let Some(prev) = last.take() {
                        init.extend(std::iter::once(prev));
                    }
                    last = Some(value);
                }
                Err(e) => return ResultSplitLastCollector(Err(e)),
            }
        }

        ResultSplitLastCollector(Ok(last.map(|last| (init, last))))
    }
}

pub mod prelude {
    use super::{ResultSplitLastCollector, SplitLastCollector};

    /// Extension trait for Iterator providing split_last adaptors.
    pub trait SplitLastItertools: Iterator {
        /// Collects iterator into initialization sequence and last element.
        fn collect_split_last<C>(self) -> Option<(C, Self::Item)>
        where
            Self: Sized,
            C: Default + Extend<Self::Item>,
        {
            self.collect::<SplitLastCollector<C, Self::Item>>()
                .into_inner()
        }

        /// Collects Result iterator into initialization sequence and last element.
        fn collect_split_last_result<C, T, E>(self) -> Result<Option<(C, T)>, E>
        where
            Self: Sized + Iterator<Item = Result<T, E>>,
            C: Default + Extend<T>,
        {
            self.collect::<ResultSplitLastCollector<C, T, E>>()
                .into_inner()
        }
    }

    impl<I: Iterator> SplitLastItertools for I {}

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_prelude() {
            let vec_result: Option<(Vec<i32>, i32)> =
                vec![1, 2, 3].into_iter().collect_split_last();
            assert_eq!(vec_result, Some((vec![1, 2], 3)));

            let result_vec: Result<Option<(Vec<i32>, i32)>, &str> = vec![Ok(1), Ok(2), Ok(3)]
                .into_iter()
                .collect_split_last_result();
            assert_eq!(result_vec, Ok(Some((vec![1, 2], 3))));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{LinkedList, VecDeque};

    #[test]
    fn test_empty() {
        let vec_result = std::iter::empty().collect::<SplitLastCollector<Vec<i32>, i32>>();
        assert_eq!(vec_result.0, None);

        let list_result = std::iter::empty().collect::<SplitLastCollector<LinkedList<i32>, i32>>();
        assert_eq!(list_result.0, None);
    }

    #[test]
    fn test_single_element() {
        let vec_result = std::iter::once(42).collect::<SplitLastCollector<Vec<i32>, i32>>();
        assert_eq!(vec_result.0, Some((vec![], 42)));

        let list_result = std::iter::once(42).collect::<SplitLastCollector<LinkedList<i32>, i32>>();
        let empty_list = LinkedList::new();
        assert_eq!(list_result.0, Some((empty_list, 42)));
    }

    #[test]
    fn test_two_elements() {
        let vec_result = vec![1, 2]
            .into_iter()
            .collect::<SplitLastCollector<Vec<i32>, i32>>();
        assert_eq!(vec_result.0, Some((vec![1], 2)));

        let deque_result = vec![1, 2]
            .into_iter()
            .collect::<SplitLastCollector<VecDeque<i32>, i32>>();
        let mut expected = VecDeque::new();
        expected.push_back(1);
        assert_eq!(deque_result.0, Some((expected, 2)));
    }

    #[test]
    fn test_many_elements() {
        let vec_result = (1..=5).collect::<SplitLastCollector<Vec<i32>, i32>>();
        assert_eq!(vec_result.0, Some((vec![1, 2, 3, 4], 5)));

        let list_result = (1..=5).collect::<SplitLastCollector<LinkedList<i32>, i32>>();
        let mut expected = LinkedList::new();
        expected.extend([1, 2, 3, 4]);
        assert_eq!(list_result.0, Some((expected, 5)));
    }

    #[test]
    fn test_with_string() {
        let string_result = "abc".chars().collect::<SplitLastCollector<String, char>>();
        assert_eq!(string_result.0, Some(("ab".to_string(), 'c')));

        let vec_result = "abc"
            .chars()
            .collect::<SplitLastCollector<Vec<char>, char>>();
        assert_eq!(vec_result.0, Some((vec!['a', 'b'], 'c')));
    }

    // Result tests
    #[test]
    fn test_result_empty() {
        let result = Vec::<Result<i32, &str>>::new()
            .into_iter()
            .collect::<ResultSplitLastCollector<Vec<i32>, i32, &str>>();
        assert_eq!(result.0, Ok(None));
    }

    #[test]
    fn test_result_single() {
        let result = vec![Ok(42)]
            .into_iter()
            .collect::<ResultSplitLastCollector<Vec<i32>, i32, &str>>();
        assert_eq!(result.0, Ok(Some((vec![], 42))));
    }

    #[test]
    fn test_result_multiple() {
        let result = vec![Ok(1), Ok(2), Ok(3)]
            .into_iter()
            .collect::<ResultSplitLastCollector<Vec<i32>, i32, &str>>();
        assert_eq!(result.0, Ok(Some((vec![1, 2], 3))));
    }

    #[test]
    fn test_result_error() {
        let result = vec![Ok(1), Err("error"), Ok(3)]
            .into_iter()
            .collect::<ResultSplitLastCollector<Vec<i32>, i32, &str>>();
        assert_eq!(result.0, Err("error"));
    }

    #[test]
    fn test_result_immediate_error() {
        let result = vec![Err("error")]
            .into_iter()
            .collect::<ResultSplitLastCollector<Vec<i32>, i32, &str>>();
        assert_eq!(result.0, Err("error"));
    }

    #[test]
    fn test_result_with_collections() {
        let input = vec![Ok(1), Ok(2), Ok(3)];

        let vec_result = input
            .clone()
            .into_iter()
            .collect::<ResultSplitLastCollector<Vec<i32>, i32, &str>>();
        assert_eq!(vec_result.0, Ok(Some((vec![1, 2], 3))));

        let list_result = input
            .clone()
            .into_iter()
            .collect::<ResultSplitLastCollector<LinkedList<i32>, i32, &str>>();
        let mut expected = LinkedList::new();
        expected.extend([1, 2]);
        assert_eq!(list_result.0, Ok(Some((expected, 3))));
    }

    #[test]
    fn test_result_with_string() {
        let input = "abc".chars().map(Ok);
        let result = input.collect::<ResultSplitLastCollector<String, char, &str>>();
        assert_eq!(result.0, Ok(Some(("ab".to_string(), 'c'))));
    }
}
