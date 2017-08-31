// https://gist.github.com/alex-shapiro/ab398f5a6a59ebf182b50fd50790d375

//! An adapter for merging the output of two streams, where
//! the stream resolves as soon either stream resolves.
extern crate futures;

use futures::{Poll, Async};
use futures::stream::{Stream, Fuse};

pub struct AndSelect<S1, S2> {
    stream1: Fuse<S1>,
    stream2: Fuse<S2>,
    flag: bool,
}

pub fn new<S1, S2>(stream1: S1, stream2: S2) -> AndSelect<S1, S2>
where
    S1: Stream,
    S2: Stream<Item = S1::Item, Error = S1::Error>,
{
    AndSelect {
        stream1: stream1.fuse(),
        stream2: stream2.fuse(),
        flag: false,
    }
}

impl<S1, S2> Stream for AndSelect<S1, S2>
where
    S1: Stream,
    S2: Stream<Item = S1::Item, Error = S1::Error>,
{
    type Item = S1::Item;
    type Error = S1::Error;

    fn poll(&mut self) -> Poll<Option<S1::Item>, S1::Error> {
        let (a, b) = if self.flag {
            (
                &mut self.stream2 as &mut Stream<Item = _, Error = _>,
                &mut self.stream1 as &mut Stream<Item = _, Error = _>,
            )
        } else {
            (
                &mut self.stream1 as &mut Stream<Item = _, Error = _>,
                &mut self.stream2 as &mut Stream<Item = _, Error = _>,
            )
        };

        match a.poll()? {
            Async::Ready(Some(item)) => {
                self.flag = !self.flag;
                return Ok(Some(item).into());
            }
            Async::Ready(None) => return Ok(None.into()),
            Async::NotReady => false,
        };

        match b.poll()? {
            Async::Ready(Some(item)) => Ok(Some(item).into()),
            Async::Ready(None) => Ok(None.into()),
            Async::NotReady => Ok(Async::NotReady),
        }
    }
}
