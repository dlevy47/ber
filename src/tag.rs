use err;
use util::TrackedReader;

use std::num::FromPrimitive;

#[derive(Debug, FromPrimitive, PartialEq, Eq)]
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
    Application(u64),
    ContextSpecific(u64),
    Private(u64),
}

#[derive(FromPrimitive)]
enum Class {
    Universal       = 0,
    Application     = 1,
    ContextSpecific = 2,
    Private         = 3,
}

#[derive(PartialEq, Eq, FromPrimitive)]
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
pub enum Length {
    Indefinite,
    Some(u64),
}

#[derive(PartialEq, Eq, Debug)]
pub struct Tag {
    pub number:  Number,
    pub length:  Length,
    pub offset:  Option<usize>,
    pub payload: Payload,
}

fn read_extended_number (r: &mut Reader) -> Result<u64, err::Error> {
    // 
    let mut count = 0us;
    let mut ret = 0u64;

    while count < 8 {
        let b = try!(r.read_u8());
        let bits = (b & 0x7F) as u64;

        ret |= bits << (7 * count);

        if b & 0x80 == 0 {
            break;
        }
        count += 1;
    }

    Ok(ret)
}

fn maybe_read_extended_number (b: u8, r: &mut Reader) -> Result<u64, err::Error> {
    if b == 0x1F {
        read_extended_number(r)
    } else {
        Ok(b as u64)
    }
}

fn read_identifiers (r: &mut Reader) -> Result<(Class, Flavor, Number), err::Error> {
    let b = try!(r.read_u8());

    // these are unwrappable because they are comprehensive within their ranges
    let class:  Class  = FromPrimitive::from_u8((b & 0xC0) >> 6).unwrap();
    let flavor: Flavor = FromPrimitive::from_u8((b & 0x20) >> 5).unwrap();
    let number = b & 0x1F;

    let number = match class {
        Class::Universal => {
            if number == 0x7F {
                // this is only valid in non-universal classes
                return Err(err::Error::new(err::Kind::InvalidTypeAndFlavor, 0, None));
            }
            Number::Universal(FromPrimitive::from_u8(number).unwrap())
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

fn read_length (r: &mut Reader) -> Result<Length, err::Error> {
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
            ret |= (b as u64) << (i * 8);
        }

        Ok(Length::Some(ret))
    } else {
        Ok(Length::Some(b as u64))
    }
}

fn read_payload(length: &Length, flavor: &Flavor, r: &mut TrackedReader) -> Result<Payload, err::Error> {
    if let &Flavor::Primitive = flavor {
        if let Length::Some(ref l) = *length {
            Ok(Payload::Primitive(try!(r.read_exact(*l as usize))))
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

impl Tag {
    fn inner_read (r: &mut TrackedReader) -> Result<Tag, err::Error> {
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
            length: length,
            offset: Some(offset),
            payload: payload,
        })
    }
    pub fn read (r: &mut Reader) -> Result<Tag, err::Error> {
        Tag::inner_read(&mut TrackedReader::new(r))
    }
}

#[cfg(test)]
mod test {
    use std::old_io::MemReader;
    use super::*;

    #[test]
    fn test_ber_read_1 () {
        let payload = vec![0x30, 0x80, 0x0C, 0x03, 0x64, 0x65, 0x66, 0x00, 0x00];
        let tag = Tag::read(&mut MemReader::new(payload)).unwrap();

        println!("{:?}", tag);

        assert!(
            tag == Tag {
                number: Number::Universal(Type::Sequence),
                length: Length::Indefinite,
                offset: Some(0),
                payload: Payload::Constructed(vec![ Tag {
                    number: Number::Universal(Type::Utf8String),
                    length: Length::Some(3),
                    offset: Some(2),
                    payload: Payload::Primitive(vec![0x64, 0x65, 0x66]),
                } ]),
            }
            );
    }

    #[test]
    fn test_ber_read_long_length () {
        let payload = vec![0x30, 0x80, 0x0C, 0x82, 0x03, 0x00, 0x64, 0x65, 0x66, 0x00, 0x00];
        let tag = Tag::read(&mut MemReader::new(payload)).unwrap();

        println!("{:?}", tag);

        assert!(
            tag == Tag {
                number: Number::Universal(Type::Sequence),
                length: Length::Indefinite,
                offset: Some(0),
                payload: Payload::Constructed(vec![ Tag {
                    number: Number::Universal(Type::Utf8String),
                    length: Length::Some(3),
                    offset: Some(2),
                    payload: Payload::Primitive(vec![0x64, 0x65, 0x66]),
                } ]),
            }
            );
    }

    #[test]
    fn test_ber_read_extended_number () {
        let payload = vec![0x30, 0x80, 0x9F, 0x0A, 0x82, 0x03, 0x00, 0x64, 0x65, 0x66, 0x00, 0x00];
        let tag = Tag::read(&mut MemReader::new(payload)).unwrap();

        println!("{:?}", tag);

        assert!(
            tag == Tag {
                number: Number::Universal(Type::Sequence),
                length: Length::Indefinite,
                offset: Some(0),
                payload: Payload::Constructed(vec![ Tag {
                    number: Number::ContextSpecific(10),
                    length: Length::Some(3),
                    offset: Some(2),
                    payload: Payload::Primitive(vec![0x64, 0x65, 0x66]),
                } ]),
            }
            );
    }
}
