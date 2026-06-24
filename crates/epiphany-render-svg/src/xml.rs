//! XML escaping and a from-scratch well-formedness validator for the SVG subset
//! the renderer emits.
//!
//! Epiphany vendors no XML library (the workspace is deliberately dependency-
//! light — every codec is hand-rolled). The renderer fully controls its output,
//! so this module validates exactly the XML constructs it produces: a single
//! root element, balanced and correctly nested tags, double-quoted attributes,
//! and `&`/`<` only as escaped entities. It is a *well-formedness* checker (not a
//! DTD/schema validator); the acceptance tests additionally cross-check output
//! with the system `xmllint` when it is available, so the claim "the SVG
//! XML-validates" rests on a real parser too, not only on this checker.

/// Escapes text content: `&`, `<`, `>` (the last for defensiveness against the
/// `]]>` sequence). Quotes are legal in text and left as-is.
pub fn escape_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

/// Escapes a double-quoted attribute value: `&`, `<`, `>`, and `"`.
pub fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// A well-formedness defect, with a human-readable reason and the byte offset.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct XmlError {
    pub reason: String,
    pub offset: usize,
}

impl std::fmt::Display for XmlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "malformed XML at byte {}: {}", self.offset, self.reason)
    }
}

/// Validates that `xml` is well-formed within the subset the renderer emits:
/// optional leading `<?xml …?>`, comments, exactly one root element, balanced
/// and properly nested tags, quoted attributes, and valid entity references.
pub fn check_well_formed(xml: &str) -> Result<(), XmlError> {
    let b = xml.as_bytes();
    let mut i = 0;
    let mut stack: Vec<&str> = Vec::new();
    // True once the first (root) element has been opened. With an empty stack
    // it means the root has closed: any further element is a second root, and
    // any non-whitespace text is stray content.
    let mut seen_any_element = false;

    let err = |offset: usize, reason: &str| {
        Err(XmlError {
            reason: reason.to_owned(),
            offset,
        })
    };

    while i < b.len() {
        if b[i] == b'<' {
            // Tag of some kind.
            if xml[i..].starts_with("<?xml") {
                if i != 0 {
                    return err(i, "XML declaration must be the first content");
                }
                let Some(end) = xml[i..].find("?>") else {
                    return err(i, "unterminated XML declaration");
                };
                i += end + 2;
                continue;
            }
            if xml[i..].starts_with("<!--") {
                let body_start = i + 4;
                let Some(rel) = xml[body_start..].find("-->") else {
                    return err(i, "unterminated comment");
                };
                if xml[body_start..body_start + rel].contains("--") {
                    return err(i, "'--' is not allowed inside a comment");
                }
                i = body_start + rel + 3;
                continue;
            }
            if xml[i..].starts_with("<!") || xml[i..].starts_with("<?") {
                return err(i, "unsupported declaration/processing-instruction");
            }
            if xml[i..].starts_with("</") {
                // Close tag.
                let name_start = i + 2;
                let Some(rel) = xml[name_start..].find('>') else {
                    return err(i, "unterminated close tag");
                };
                let name = xml[name_start..name_start + rel].trim_end();
                if !is_name(name) {
                    return err(i, "invalid element name in close tag");
                }
                match stack.pop() {
                    Some(open) if open == name => {}
                    Some(open) => {
                        return err(
                            i,
                            &format!("close tag </{name}> does not match open <{open}>"),
                        )
                    }
                    None => return err(i, &format!("close tag </{name}> with no open element")),
                }
                i = name_start + rel + 1;
                continue;
            }
            // Open or self-closing tag: parse name then attributes.
            let (name, mut j) = read_name(xml, i + 1).ok_or_else(|| XmlError {
                reason: "invalid element name".to_owned(),
                offset: i,
            })?;
            if stack.is_empty() && seen_any_element {
                return err(i, "more than one root element");
            }
            seen_any_element = true;
            // Attributes.
            loop {
                j = skip_ws(b, j);
                if j >= b.len() {
                    return err(i, "unterminated start tag");
                }
                if b[j] == b'>' {
                    stack.push(name);
                    j += 1;
                    break;
                }
                if b[j] == b'/' {
                    if j + 1 < b.len() && b[j + 1] == b'>' {
                        // Self-closing: opens and closes in place; one element.
                        j += 2;
                        break;
                    }
                    return err(j, "'/' not followed by '>'");
                }
                // An attribute: name (=) "value".
                let (attr, after_name) = read_name(xml, j).ok_or_else(|| XmlError {
                    reason: "invalid attribute name".to_owned(),
                    offset: j,
                })?;
                let _ = attr;
                let k = skip_ws(b, after_name);
                if k >= b.len() || b[k] != b'=' {
                    return err(k.min(b.len()), "attribute missing '='");
                }
                let k = skip_ws(b, k + 1);
                if k >= b.len() || (b[k] != b'"' && b[k] != b'\'') {
                    return err(k.min(b.len()), "attribute value must be quoted");
                }
                let quote = b[k];
                let val_start = k + 1;
                let mut m = val_start;
                while m < b.len() && b[m] != quote {
                    if b[m] == b'<' {
                        return err(m, "'<' not allowed in attribute value");
                    }
                    if b[m] == b'&' {
                        m = check_entity(xml, m)?;
                        continue;
                    }
                    m += 1;
                }
                if m >= b.len() {
                    return err(val_start, "unterminated attribute value");
                }
                j = m + 1; // past the closing quote
            }
            i = j;
        } else {
            // Text content. Outside the root (before it opens or after it
            // closes) only whitespace is permitted; inside, entities must be
            // valid and a raw '<' would already have been taken as a tag.
            if stack.is_empty() {
                if !b[i].is_ascii_whitespace() {
                    return err(
                        i,
                        if seen_any_element {
                            "text after the root element"
                        } else {
                            "text before the root element"
                        },
                    );
                }
                i += 1;
                continue;
            }
            if b[i] == b'&' {
                i = check_entity(xml, i)?;
                continue;
            }
            i += 1;
        }
    }

    if !stack.is_empty() {
        return Err(XmlError {
            reason: format!("unclosed element <{}>", stack.last().unwrap()),
            offset: xml.len(),
        });
    }
    if !seen_any_element {
        return Err(XmlError {
            reason: "no root element".to_owned(),
            offset: 0,
        });
    }
    Ok(())
}

/// Validates an entity reference starting at `start` (`b[start] == '&'`),
/// returning the offset just past its terminating `;`.
fn check_entity(xml: &str, start: usize) -> Result<usize, XmlError> {
    let rest = &xml[start..];
    let Some(semi) = rest.find(';') else {
        return Err(XmlError {
            reason: "entity reference missing ';'".to_owned(),
            offset: start,
        });
    };
    let body = &rest[1..semi];
    let ok = matches!(body, "amp" | "lt" | "gt" | "quot" | "apos")
        || (body.starts_with("#x")
            && body.len() > 2
            && body[2..].bytes().all(|c| c.is_ascii_hexdigit()))
        || (body.starts_with('#')
            && body.len() > 1
            && !body.starts_with("#x")
            && body[1..].bytes().all(|c| c.is_ascii_digit()));
    if !ok {
        return Err(XmlError {
            reason: format!("invalid entity reference &{body};"),
            offset: start,
        });
    }
    Ok(start + semi + 1)
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// Reads an XML name starting at `start`, returning `(name, offset_past_name)`.
fn read_name(xml: &str, start: usize) -> Option<(&str, usize)> {
    let b = xml.as_bytes();
    if start >= b.len() || !is_name_start(b[start]) {
        return None;
    }
    let mut i = start + 1;
    while i < b.len() && is_name_char(b[i]) {
        i += 1;
    }
    Some((&xml[start..i], i))
}

fn is_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty() && is_name_start(b[0]) && b[1..].iter().all(|&c| is_name_char(c))
}

fn is_name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_' || c == b':'
}

fn is_name_char(c: u8) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == b'-' || c == b'.'
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_well_formed_documents() {
        check_well_formed(r#"<svg xmlns="x"><g><path d="M0 0Z"/></g></svg>"#).unwrap();
        check_well_formed("<?xml version=\"1.0\"?>\n<a><!-- c --><b x='1'/></a>\n").unwrap();
        check_well_formed(r#"<a t="&amp;&lt;&#48;&#x2F;"/>"#).unwrap();
    }

    #[test]
    fn rejects_mismatched_and_unclosed_tags() {
        assert!(check_well_formed("<a><b></a></b>").is_err());
        assert!(check_well_formed("<a><b></b>").is_err());
        assert!(check_well_formed("<a>").is_err());
        assert!(check_well_formed("</a>").is_err());
    }

    #[test]
    fn rejects_two_roots_and_stray_markup() {
        assert!(check_well_formed("<a/><b/>").is_err());
        assert!(check_well_formed("<a>&bogus;</a>").is_err());
        assert!(check_well_formed(r#"<a x=1/>"#).is_err());
        assert!(check_well_formed(r#"<a x="1<2"/>"#).is_err());
        assert!(check_well_formed("text<a/>").is_err());
    }

    #[test]
    fn escaping_is_correct_and_round_trips_through_the_checker() {
        let t = escape_text("a & b < c > d");
        assert_eq!(t, "a &amp; b &lt; c &gt; d");
        let a = escape_attr(r#"x"&<y"#);
        assert_eq!(a, "x&quot;&amp;&lt;y");
        check_well_formed(&format!("<n title=\"{a}\">{t}</n>")).unwrap();
    }
}
