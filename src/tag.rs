use std::io::{self, Write, Read};
use std::num::FromPrimitive;

use byteorder::{self, ReadBytesExt, WriteBytesExt};

use err;
use util::TrackedRead;

#[derive(Debug, FromPrimitive, PartialEq, Eq, Copy)]
pub enum Type {
    Eoc              = 0,
    Boolean          = 1,
    Integer          = 2,
    BitString        = 3,
    OctetString      = 4,
    Null             = 5,
    ObjectIdentifier = 6,
    ObjectDescriptor = 7,
    External         = 8,
    Real             = 9,
    Enumerated       = 10,
    EmbeddedPdv      = 11,
    Utf8String       = 12,
    RelativeOid      = 13,
    Sequence         = 16,
    Set              = 17,
    NumericString    = 18,
    PrintableString  = 19,
    T61String        = 20,
    VideotexString   = 21,
    Ia5String        = 22,
    UtcTime          = 23,
    GeneralizedTime  = 24,
    GraphicString    = 25,
    VisibleString    = 26,
    GeneralString    = 27,
    UniversalString  = 28,
    CharacterString  = 29,
    BmpString        = 30,
}

#[derive(PartialEq, Eq, Debug)]
pub enum Number {
    Universal(Type),
    Application(i64),
    ContextSpecific(i64),
    Private(i64),
}

#[derive(Debug, FromPrimitive, Copy)]
enum Class {
    Universal       = 0,
    Application     = 1,
    ContextSpecific = 2,
    Private         = 3,
}

#[derive(Debug, PartialEq, Eq, FromPrimitive, Copy)]
enum Flavor {
    Primitive   = 0,
    Constructed = 1,
}

#[derive(PartialEq, Eq, Debug)]
pub enum Payload {
    Primitive(Vec<u8>),
    Constructed(Vec<Tag>),
}

#[derive(PartialEq, Eq, Debug)]
enum Length {
    Indefinite,
    Some(u64),
}

#[derive(PartialEq, Eq, Debug)]
pub struct Tag {
    pub number:  Number,
    pub offset:  Option<usize>,
    pub payload: Payload,
}

fn read_extended_number (mut r: &mut Read) -> Result<i64, err::Error> {
    // 
    let mut count = 0usize;
    let mut ret = 0i64;

    while count < 8 {
        let b = try!(r.read_u8());
        let bits = (b & 0x7F) as i64;

        ret |= bits << (7 * count);

        if b & 0x80 == 0 {
            break;
        }
        count += 1;
    }

    Ok(ret)
}

fn maybe_read_extended_number (b: i8, r: &mut Read) -> Result<i64, err::Error> {
    if b == 0x1F {
        read_extended_number(r)
    } else {
        Ok(b as i64)
    }
}

fn read_identifiers (mut r: &mut Read) -> Result<(Class, Flavor, Number), err::Error> {
    let b = try!(r.read_u8());

    // these are unwrappable because they are comprehensive within their ranges
    let class:  Class  = FromPrimitive::from_u8((b & 0xC0) >> 6).unwrap();
    let flavor: Flavor = FromPrimitive::from_u8((b & 0x20) >> 5).unwrap();
    let number = (b & 0x1F) as i8;

    let number = match class {
        Class::Universal => {
            if number == 0x1F {
                // this is only valid in non-universal classes
                return Err(err::Error::new(err::Kind::InvalidTypeAndFlavor, 0, None));
            }
            Number::Universal(FromPrimitive::from_i8(number).unwrap())
        },
        Class::Application =>
            Number::Application(try!(maybe_read_extended_number(number, r))),
        Class::ContextSpecific =>
            Number::ContextSpecific(try!(maybe_read_extended_number(number, r))),
        Class::Private =>
            Number::Private(try!(maybe_read_extended_number(number, r))),
    };

    Ok((class, flavor, number))
}

fn read_length (mut r: &mut Read) -> Result<Length, err::Error> {
    let b = try!(r.read_u8());

    if b == 0x80 {
        Ok(Length::Indefinite)
    } else if b & 0x80 == 0x80 {
        // long form
        let mut ret = 0u64;

        let count = (b & 0x7F) as usize;

        if count > 8 {
            // too big for us
            return Err(err::Error::new(err::Kind::NumberOverflow, 0, None));
        }

        for i in 0..count {
            let b = try!(r.read_u8());
            ret |= (b as u64) << ((count - i -1) * 8);
        }

        Ok(Length::Some(ret))
    } else {
        Ok(Length::Some(b as u64))
    }
}

fn read_payload(length: &Length, flavor: &Flavor, mut r: &mut TrackedRead) -> Result<Payload, err::Error> {
    if let &Flavor::Primitive = flavor {
        if let Length::Some(ref l) = *length {
            let mut buf = vec![0; *l as usize];
            //TODO: handle partial reads?
            try!(r.read(&mut buf));
            Ok(Payload::Primitive(buf))
        } else {
            unreachable!()
        }
    } else {
        let start = r.tell();
        let mut children = Vec::new();

        while {
            let child = try!(Tag::inner_read(r));

            if child.number == Number::Universal(Type::Eoc) && *length == Length::Indefinite {
                // this is the end of the indefinite constructed payload
                false
            } else {
                children.push(child);
                if let Length::Some(ref l) = *length {
                    if r.tell() - start >= *l as usize {
                        false
                    } else {
                        true
                    }
                } else {
                    true
                }
            }
        } {}

        Ok(Payload::Constructed(children))
    }
}

fn write_extended_number (mut w: &mut Write, mut num: i64) -> io::Result<()> {
    let mask = 0x7F;

    while num > 0 {
        let mut b = (num & mask) as u8;

        num >>= 7;
        if num != 0 {
            b |= 0x80;
        }

        try!(w.write_u8(b));
    }
    Ok(())
}

fn maybe_write_extended_number (w: &mut Write, num: i64) -> io::Result<()> {
    if num >= 0x1F {
        write_extended_number(w, num)
    } else {
        Ok(())
    }
}

fn write_identifiers (mut w: &mut Write, class: &Class, flavor: &Flavor, number: &Number) -> io::Result<()> {
    let b: u8 = 
        (*class as u8)  << 6 |
        (*flavor as u8) << 5 |
        match *number {
            Number::Universal(ref t) => (*t as u8),
            Number::Application(ref n) |
                Number::ContextSpecific(ref n) |
                Number::Private(ref n) => if *n >= 0x1F {
                    0x1F
                } else {
                    *n as u8
                }
        };

    try!(w.write_u8(b));
    match *number {
        Number::Application(ref num) |
            Number::ContextSpecific(ref num) |
            Number::Private(ref num) => try!(maybe_write_extended_number(w, *num)),
            _ => {},
    }

    Ok(())
}

fn write_length (mut w: &mut Write, length: &Length) -> byteorder::Result<()> {
    match length {
        &Length::Indefinite => w.write_u8(0x80),
        &Length::Some(ref l) => {
            if *l < 0x1F {
                w.write_u8(*l as u8)
            } else {
                let count = {
                    let mut count = 0;
                    let mut val = *l;

                    while {
                        count += 1;
                        val >>= 8;
                        val > 0
                    } {}
                    count
                } as u8;

                try!(w.write_u8(count | 0x80));

                for i in (0..count).rev() {
                    // start with the largest bytes first
                    let byte = ((*l & (0xFF << i * 8)) >> i * 8) as u8;
                    try!(w.write_u8(byte));
                }

                Ok(())
            }
        },
    }
}

fn write_payload (mut w: &mut Write, payload: &Payload) -> io::Result<()> {
    match payload {
        &Payload::Primitive(ref v) => {
            w.write_all(v)
        },
        &Payload::Constructed(ref v) => {
            for tag in v {
                try!(tag.write(w));
            }
            Ok(())
        },
    }
}

impl Tag {
    fn inner_read (r: &mut TrackedRead) -> Result<Tag, err::Error> {
        let offset = r.tell();

        let (_class, flavor, number) = match read_identifiers(r) {
            Ok(x) => x,
            Err(mut e) => {
                e.offset = r.tell();
                return Err(e);
            },
        };

        let length = match read_length(r) {
            Ok(x) => x,
            Err(mut e) => {
                e.offset = r.tell();
                return Err(e);
            },
        };

        println!("found {:?} {:?} {:?} {:?}", _class, flavor, number, length);

        if length == Length::Indefinite  && flavor == Flavor::Primitive {
            return Err(err::Error::new(err::Kind::InvalidLength, r.tell(), None));
        }

        let payload = match read_payload(&length, &flavor, r) {
            Ok(x) => x,
            Err(mut e) => {
                e.offset = r.tell();
                return Err(e);
            },
        };

        Ok(Tag {
            number: number,
            offset: Some(offset),
            payload: payload,
        })
    }
    pub fn read (r: &mut Read) -> Result<Tag, err::Error> {
        Tag::inner_read(&mut TrackedRead::new(r))
    }

    pub fn write (&self, mut w: &mut Write) -> io::Result<()> {
        let class = match self.number {
            Number::Universal(_) => Class::Universal,
            Number::Application(_) => Class::Application,
            Number::ContextSpecific(_) => Class::ContextSpecific,
            Number::Private(_) => Class::Private,
        };

        let (flavor, length) = match self.payload {
            Payload::Primitive(ref v) => (Flavor::Primitive, Length::Some(v.len() as u64)),
            Payload::Constructed(_) => (Flavor::Constructed, Length::Indefinite),
        };

        try!(write_identifiers(w, &class, &flavor, &self.number));

        try!(write_length(w, &length));

        try!(write_payload(w, &self.payload));

        match length {
            Length::Indefinite => w.write_all(&[0x00, 0x00]),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod test {
    use std::io::Cursor;
    use super::*;

    #[test]
    fn test_ber_read_1 () {
        let payload = vec![0x30, 0x80, 0x0C, 0x03, 0x64, 0x65, 0x66, 0x00, 0x00];
        let tag = Tag::read(&mut Cursor::new(payload)).unwrap();

        println!("{:?}", tag);

        assert!(
            tag == Tag {
                number: Number::Universal(Type::Sequence),
                offset: Some(0),
                payload: Payload::Constructed(vec![ Tag {
                    number: Number::Universal(Type::Utf8String),
                    offset: Some(2),
                    payload: Payload::Primitive(vec![0x64, 0x65, 0x66]),
                } ]),
            }
            );
    }

    #[test]
    fn test_ber_write_1 () {
        let payload = vec![0x30, 0x80, 0x0C, 0x03, 0x64, 0x65, 0x66, 0x00, 0x00];
        let tag = Tag::read(&mut Cursor::new(payload.clone())).unwrap();


        let mut buf = Vec::<u8>::new();
        tag.write(&mut buf).unwrap();
        assert!(buf == payload);
    }

    #[test]
    fn test_ber_read_long_length () {
        let payload = vec![0x30, 0x80, 0x0C, 0x82, 0x00, 0x03, 0x64, 0x65, 0x66, 0x00, 0x00];
        let tag = Tag::read(&mut Cursor::new(payload)).unwrap();

        println!("{:?}", tag);

        assert!(
            tag == Tag {
                number: Number::Universal(Type::Sequence),
                offset: Some(0),
                payload: Payload::Constructed(vec![ Tag {
                    number: Number::Universal(Type::Utf8String),
                    offset: Some(2),
                    payload: Payload::Primitive(vec![0x64, 0x65, 0x66]),
                } ]),
            }
            );
    }

    #[test]
    fn test_ber_write_long_length () {
        let payload = vec![0x30, 0x80, 0x0C, 0x03, 0x64, 0x65, 0x66, 0x00, 0x00];
        let tag = Tag::read(&mut Cursor::new(payload.clone())).unwrap();


        let mut buf = Vec::<u8>::new();
        tag.write(&mut buf).unwrap();
        assert!(buf == payload);
    }

    #[test]
    fn test_ber_read_extended_number () {
        let payload = vec![0x30, 0x80, 0x9F, 0x7F, 0x81, 0x03, 0x64, 0x65, 0x66, 0x00, 0x00];
        let tag = Tag::read(&mut Cursor::new(payload)).unwrap();

        println!("{:?}", tag);

        assert!(
            tag == Tag {
                number: Number::Universal(Type::Sequence),
                offset: Some(0),
                payload: Payload::Constructed(vec![ Tag {
                    number: Number::ContextSpecific(0x7F),
                    offset: Some(2),
                    payload: Payload::Primitive(vec![0x64, 0x65, 0x66]),
                } ]),
            }
            );
    }

    #[test]
    fn test_ber_write_extended_number () {
        let payload = vec![0x30, 0x80, 0x9F, 0x7F, 0x03, 0x64, 0x65, 0x66, 0x00, 0x00];
        let tag = Tag::read(&mut Cursor::new(payload.clone())).unwrap();


        let mut buf = Vec::<u8>::new();
        tag.write(&mut buf).unwrap();
        assert!(buf == payload);
    }

    #[test]
    #[should_panic]
    fn test_invalid_number () {
        let payload = vec![0x3F];
        let _tag = Tag::read(&mut Cursor::new(payload.clone())).unwrap();
    }

}
