use git_repository::bstr::ByteSlice;

use crate::{
    changelog,
    changelog::{
        section,
        section::{segment, Segment},
        Section,
    },
    ChangeLog,
};

impl std::fmt::Display for changelog::Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            changelog::Version::Unreleased => f.write_str("Unreleased"),
            changelog::Version::Semantic(v) => write!(f, "v{}", v),
        }
    }
}

impl Section {
    pub const UNKNOWN_TAG_START: &'static str = "<csr-unknown>";
    pub const UNKNOWN_TAG_END: &'static str = "<csr-unknown/>";
    pub const READONLY_TAG: &'static str = "<csr-read-only-do-not-edit/>\n"; // needs a newline to not interfere with formatting

    pub fn write_to(&self, mut out: impl std::io::Write) -> std::io::Result<()> {
        match self {
            Section::Verbatim { text, .. } => out.write_all(text.as_bytes()),
            Section::Release {
                name,
                date,
                heading_level,
                segments,
                removed_messages,
                unknown,
            } => {
                write!(out, "{} {}", heading(*heading_level), name)?;
                match date {
                    None => out.write_all(b"\n\n"),
                    Some(date) => writeln!(
                        out,
                        " ({:04}-{:02}-{:02})\n",
                        date.year(),
                        date.month() as u32,
                        date.day()
                    ),
                }?;
                if !removed_messages.is_empty() {
                    for id in removed_messages {
                        writeln!(out, "{}{}/>", segment::Conventional::REMOVED_HTML_PREFIX, id)?;
                    }
                    writeln!(out)?;
                }

                let section_level = *heading_level + 1;
                for segment in segments {
                    segment.write_to(section_level, &mut out)?;
                }
                if !unknown.is_empty() {
                    writeln!(out, "{}", Section::UNKNOWN_TAG_START)?;
                    out.write_all(unknown.as_bytes())?;
                    writeln!(out, "{}", Section::UNKNOWN_TAG_END)?;
                }
                Ok(())
            }
        }
    }
}

fn heading(level: usize) -> String {
    "#".repeat(level)
}

impl ChangeLog {
    pub fn write_to(&self, mut out: impl std::io::Write) -> std::io::Result<()> {
        for section in &self.sections {
            section.write_to(&mut out)?;
        }
        Ok(())
    }
}

impl section::Segment {
    pub fn write_to(&self, section_level: usize, mut out: impl std::io::Write) -> std::io::Result<()> {
        match self {
            Segment::User { markdown } => out.write_all(markdown.as_bytes())?,
            Segment::Conventional(segment::Conventional {
                kind,
                is_breaking,
                removed,
                messages,
            }) => match segment::conventional::as_headline(kind).or_else(|| is_breaking.then(|| *kind)) {
                Some(headline) => {
                    writeln!(
                        out,
                        "{} {}{}\n",
                        heading(section_level),
                        headline,
                        if *is_breaking {
                            format!(" {}", segment::Conventional::BREAKING_TITLE_ENCLOSED)
                        } else {
                            "".into()
                        },
                    )?;

                    if !removed.is_empty() {
                        for id in removed {
                            writeln!(out, "{}{}/>", segment::Conventional::REMOVED_HTML_PREFIX, id)?;
                        }
                        writeln!(out)?;
                    }

                    use segment::conventional::Message;
                    for message in messages {
                        match message {
                            Message::Generated { title, id, body } => {
                                writeln!(
                                    out,
                                    " - {}{}/> {}",
                                    segment::Conventional::REMOVED_HTML_PREFIX,
                                    id,
                                    title
                                )?;
                                if let Some(body) = body {
                                    for line in body.as_bytes().as_bstr().lines_with_terminator() {
                                        write!(out, "   {}", line.to_str().expect("cannot fail as original is UTF-8"))?;
                                    }
                                    if !body.ends_with('\n') {
                                        writeln!(out)?;
                                    }
                                }
                            }
                            Message::User { markdown } => {
                                out.write_all(markdown.as_bytes())?;
                                if !markdown.ends_with('\n') {
                                    writeln!(out)?;
                                }
                            }
                        }
                    }
                    writeln!(out)?;
                }
                None => log::trace!(
                    "Skipping unknown git-conventional kind {:?} and all {} message(s) in it.",
                    kind,
                    messages.len()
                ),
            },
            Segment::Details(section::Data::Generated(segment::Details { commits_by_category }))
                if !commits_by_category.is_empty() =>
            {
                writeln!(out, "{} {}\n", heading(section_level), segment::Details::TITLE)?;
                writeln!(out, "{}", Section::READONLY_TAG)?;
                writeln!(out, "{}\n", segment::Details::PREFIX)?;
                for (category, messages) in commits_by_category.iter() {
                    writeln!(out, " * **{}**", category)?;
                    for message in messages {
                        writeln!(out, "    - {} ({})", message.title, message.id.to_hex(7))?;
                    }
                }
                writeln!(out, "{}\n", segment::Details::END)?;
            }
            Segment::Statistics(section::Data::Generated(segment::CommitStatistics {
                count,
                duration,
                conventional_count,
                unique_issues,
            })) => {
                writeln!(out, "{} {}\n", heading(section_level), segment::CommitStatistics::TITLE)?;
                writeln!(out, "{}", Section::READONLY_TAG)?;
                writeln!(
                    out,
                    " - {} {} contributed to the release{}",
                    count,
                    if *count == 1 { "commit" } else { "commits" },
                    match duration {
                        Some(duration) if duration.whole_days() > 0 => format!(
                            " over the course of {} calendar {}.",
                            duration.whole_days(),
                            if duration.whole_days() == 1 { "day" } else { "days" }
                        ),
                        _ => ".".into(),
                    }
                )?;
                writeln!(
                    out,
                    " - {} {} where understood as [conventional](https://www.conventionalcommits.org).",
                    conventional_count,
                    if *conventional_count == 1 { "commit" } else { "commits" }
                )?;
                if unique_issues.is_empty() {
                    writeln!(out, " - 0 issues like '(#ID)' where seen in commit messages")?;
                } else {
                    writeln!(
                        out,
                        " - {} unique {} {} worked on: {}",
                        unique_issues.len(),
                        if unique_issues.len() == 1 { "issue" } else { "issues" },
                        if unique_issues.len() == 1 { "was" } else { "were" },
                        unique_issues
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    )?;
                }
                writeln!(out)?;
            }
            Segment::Clippy(section::Data::Generated(segment::ThanksClippy { count })) if *count > 0 => {
                writeln!(out, "{} {}\n", heading(section_level), segment::ThanksClippy::TITLE)?;
                writeln!(out, "{}", Section::READONLY_TAG)?;
                writeln!(
                    out,
                    "[Clippy](https://github.com/rust-lang/rust-clippy) helped {} {} to make code idiomatic. \n",
                    count,
                    if *count > 1 { "times" } else { "time" }
                )?;
            }
            Segment::Clippy(_) => {}
            Segment::Statistics(_) => {}
            Segment::Details(_) => {}
        };
        Ok(())
    }
}