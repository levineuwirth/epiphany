use epiphany_textproj::COMPANION_VERSION;

fn title_page_version(source: &str) -> Result<(u32, u32, u32), String> {
    let title_page = source
        .split_once("\\begin{titlepage}")
        .and_then(|(_, after_start)| after_start.split_once("\\end{titlepage}"))
        .map(|(title_page, _)| title_page)
        .ok_or_else(|| "companion must contain one complete titlepage environment".to_owned())?;

    let candidates: Vec<_> = title_page
        .lines()
        .filter_map(|line| line.split_once("Version ").map(|(_, suffix)| suffix))
        .collect();
    if candidates.len() != 1 {
        return Err(format!(
            "title page must contain exactly one `Version <major>.<minor>.<patch>` anchor; found {}",
            candidates.len()
        ));
    }

    let token: String = candidates[0]
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect();
    let components: Vec<_> = token.split('.').collect();
    if components.len() != 3 || components.iter().any(|component| component.is_empty()) {
        return Err(format!(
            "title-page version must have exactly three numeric components; found `{token}`"
        ));
    }

    let parse_component = |component: &str| {
        component
            .parse::<u32>()
            .map_err(|_| format!("invalid numeric component `{component}` in title-page version"))
    };
    Ok((
        parse_component(components[0])?,
        parse_component(components[1])?,
        parse_component(components[2])?,
    ))
}

#[test]
fn companion_version_matches_title_page() {
    let companion_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../spec/text_projection.tex"
    );
    let companion = std::fs::read_to_string(companion_path)
        .unwrap_or_else(|error| panic!("failed to read {companion_path}: {error}"));
    let title_version = title_page_version(&companion)
        .unwrap_or_else(|error| panic!("failed to parse {companion_path}: {error}"));

    assert_eq!(
        title_version, COMPANION_VERSION,
        "title-page companion version differs from epiphany_textproj::COMPANION_VERSION"
    );
}
