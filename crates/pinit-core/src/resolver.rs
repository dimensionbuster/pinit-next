//! Resolver composition. Concrete resolvers live in adapters; the core only
//! knows how to chain them.

use crate::image_ref::ImageRef;
use crate::ports::{ImageResolver, ResolveError, ResolvedImage};

/// Tries each resolver in order. `Unsupported` falls through silently; real
/// failures are remembered and the last one is reported if nothing succeeds.
pub struct ChainResolver {
    links: Vec<Box<dyn ImageResolver>>,
}

impl ChainResolver {
    pub fn new(links: Vec<Box<dyn ImageResolver>>) -> Self {
        Self { links }
    }
}

impl ImageResolver for ChainResolver {
    fn resolve(&self, image: &ImageRef) -> Result<ResolvedImage, ResolveError> {
        let mut last: Option<ResolveError> = None;
        for link in &self.links {
            match link.resolve(image) {
                Ok(r) => return Ok(r),
                Err(ResolveError::Unsupported) => {}
                Err(e) => last = Some(e),
            }
        }
        Err(last.unwrap_or(ResolveError::Unsupported))
    }
}

#[cfg(test)]
pub(crate) mod fakes {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    pub struct Fixed {
        pub answer: Result<(u32, u32), ResolveError>,
        pub calls: Rc<Cell<u32>>,
    }

    impl ImageResolver for Fixed {
        fn resolve(&self, image: &ImageRef) -> Result<ResolvedImage, ResolveError> {
            self.calls.set(self.calls.get() + 1);
            self.answer
                .clone()
                .map(|(width, height)| ResolvedImage {
                    image: image.clone(),
                    local_path: "fake.png".into(),
                    width,
                    height,
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fakes::Fixed;
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    fn img() -> ImageRef {
        ImageRef::Url { url: "https://e.com/a.jpg".into(), referer: None }
    }

    fn link(answer: Result<(u32, u32), ResolveError>) -> (Box<dyn ImageResolver>, Rc<Cell<u32>>) {
        let calls = Rc::new(Cell::new(0));
        (Box::new(Fixed { answer, calls: calls.clone() }), calls)
    }

    #[test]
    fn first_success_wins_and_stops_the_chain() {
        let (a, ca) = link(Ok((10, 20)));
        let (b, cb) = link(Ok((99, 99)));
        let r = ChainResolver::new(vec![a, b]).resolve(&img()).unwrap();
        assert_eq!((r.width, r.height), (10, 20));
        assert_eq!((ca.get(), cb.get()), (1, 0));
    }

    #[test]
    fn unsupported_and_failures_fall_through() {
        let (a, _) = link(Err(ResolveError::Unsupported));
        let (b, _) = link(Err(ResolveError::Failed("net".into())));
        let (c, _) = link(Ok((4, 3)));
        assert!(ChainResolver::new(vec![a, b, c]).resolve(&img()).is_ok());
    }

    #[test]
    fn reports_last_real_failure_otherwise_unsupported() {
        let (a, _) = link(Err(ResolveError::Failed("first".into())));
        let (b, _) = link(Err(ResolveError::Failed("second".into())));
        assert_eq!(
            ChainResolver::new(vec![a, b]).resolve(&img()),
            Err(ResolveError::Failed("second".into()))
        );
        let (u, _) = link(Err(ResolveError::Unsupported));
        assert_eq!(ChainResolver::new(vec![u]).resolve(&img()), Err(ResolveError::Unsupported));
        assert_eq!(ChainResolver::new(vec![]).resolve(&img()), Err(ResolveError::Unsupported));
    }
}
