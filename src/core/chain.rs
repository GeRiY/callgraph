//! Generikus Chain of Responsibility: a szabályok sorban futnak, az első illeszkedő dönt.
//!
//! A `Rule<I>` egy láncszem az `I` bemenetre; a `Chain<R, O>` a (jellemzően `dyn`) szabály-típus
//! `R` rendezett listája és egy tartalék érték `O`. Az `R` `?Sized`, így a bemenet lifetime-paraméteres
//! is lehet (`dyn for<'a> Rule<Bemenet<'a>>`), ahogy a PHP hívás-kategorizáló használja.

/// A lánc egy láncszeme: illeszkedés esetén eredményt ad, különben `None`-t.
pub trait Rule<I: ?Sized> {
    type Output;

    fn apply(&self, input: &I) -> Option<Self::Output>;
}

/// Rendezett szabálylista (a sorrend a prioritás) tartalék értékkel.
pub struct Chain<R: ?Sized, O> {
    rules: Vec<Box<R>>,
    fallback: O,
}

impl<R: ?Sized, O: Clone> Chain<R, O> {
    pub fn new(rules: Vec<Box<R>>, fallback: O) -> Self {
        Self { rules, fallback }
    }

    /// Az első illeszkedő szabály eredménye; ha egy sem illik: a tartalék érték.
    pub fn resolve<I: ?Sized>(&self, input: &I) -> O
    where
        R: Rule<I, Output = O>,
    {
        self.rules
            .iter()
            .find_map(|r| r.apply(input))
            .unwrap_or_else(|| self.fallback.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Even;
    impl Rule<u32> for Even {
        type Output = &'static str;
        fn apply(&self, n: &u32) -> Option<&'static str> {
            n.is_multiple_of(2).then_some("even")
        }
    }

    struct Big;
    impl Rule<u32> for Big {
        type Output = &'static str;
        fn apply(&self, n: &u32) -> Option<&'static str> {
            (*n > 10).then_some("big")
        }
    }

    fn chain() -> Chain<dyn Rule<u32, Output = &'static str>, &'static str> {
        Chain::new(vec![Box::new(Big), Box::new(Even)], "other")
    }

    #[test]
    fn first_matching_rule_wins() {
        assert_eq!(chain().resolve(&12), "big");
        assert_eq!(chain().resolve(&4), "even");
    }

    #[test]
    fn falls_back_when_nothing_matches() {
        assert_eq!(chain().resolve(&3), "other");
    }
}
