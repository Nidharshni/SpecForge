use super::{page, page_args, table};
use crate::queries::{self, ListEntry, ListFilter, ListKind};
use specforge_extension_sdk::prelude::{
    CommandCall, CommandError, CommandOutput, ContributionsBuilder,
};
use std::fmt::Write as _;

pub(super) fn declare(c: &mut ContributionsBuilder) {
    list_command::<queries::FeatureListEntry>(
        c,
        "List features: each feature's id, title, status, priority, \
            problem and tags, filtered (every filter set must match), sorted \
            (by id unless --sort-by says otherwise) and paged, with the \
            total before paging.",
    );
    list_command::<queries::JourneyListEntry>(
        c,
        "List journeys: each journey's id, title, persona, channel and \
            feature counts, priority and tags, filtered (every filter set \
            must match), sorted (by id unless --sort-by says otherwise) and \
            paged, with the total before paging.",
    );
    list_command::<queries::DeliverableListEntry>(
        c,
        "List deliverables: each deliverable's id, title, artifact type, \
            status, journey and module counts and tags, filtered (every \
            filter set must match), sorted (by id unless --sort-by says \
            otherwise) and paged, with the total before paging.",
    );
    list_command::<queries::MilestoneListEntry>(
        c,
        "List milestones: each milestone's id, title, status, target \
            date, feature count, priority and tags, filtered (every filter \
            set must match), sorted (by id unless --sort-by says otherwise) \
            and paged, with the total before paging.",
    );
    list_command::<queries::ModuleListEntry>(
        c,
        "List modules: each module's id, title, family, feature count, \
            the modules it depends on and tags, filtered (every filter set \
            must match), sorted (by id unless --sort-by says otherwise) and \
            paged, with the total before paging.",
    );
    list_command::<queries::TermListEntry>(
        c,
        "List terms: each term's id, title, definition, alias count and \
            tags, filtered (every filter set must match), sorted (by id \
            unless --sort-by says otherwise) and paged, with the total \
            before paging.",
    );
    list_command::<queries::PersonaListEntry>(
        c,
        "List personas: each persona's id, title, technical level, \
            status, journey count and tags, filtered (every filter set must \
            match), sorted (by id unless --sort-by says otherwise) and \
            paged, with the total before paging.",
    );
    list_command::<queries::ChannelListEntry>(
        c,
        "List channels: each channel's id, title, interaction model, \
            status, journey count and tags, filtered (every filter set must \
            match), sorted (by id unless --sort-by says otherwise) and \
            paged, with the total before paging.",
    );
    list_command::<queries::ReleaseListEntry>(
        c,
        "List releases: each release's id, title, version, status, \
            deliverable count, release date and tags, filtered (every filter \
            set must match), sorted (by id unless --sort-by says otherwise) \
            and paged, with the total before paging.",
    );
}

/// The list command over `T`'s kind, `specforge product <plural>`: its
/// filter args are the kind's [`ListKind::filters`], in order, each with its
/// help in `filter_help`, then the tags, sort and page args every list takes.
fn list_command<T: ListEntry + 'static>(c: &mut ContributionsBuilder, description: &str) {
    let kind = T::KIND;
    c.command(kind.plural, |cmd| {
        cmd.title(&format!("List {}", kind.plural))
            .description(description)
            .category("query");
        // A closed filter is `one_of` its field's vocabulary (the one the
        // field is declared an enum of): the SDK refuses any other value, and
        // the CLI help and the MCP tool's schema list them.
        for filter in kind.filters {
            cmd.arg(filter.field(), |a| {
                if let Some(vocabulary) = kind.vocabulary(filter.field()) {
                    a.one_of(vocabulary.values());
                }
                a.description(&filter.help(kind));
            });
        }
        cmd.arg("tags", |a| {
            a.description(&format!(
                "Only {} with at least one of these tags (comma-separated)",
                kind.plural
            ));
        })
        .arg("sort_by", |a| {
            a.description(&format!(
                "The field to sort by (default id): id, title or a field a {} declares; ties by id",
                kind.kind
            ));
        })
        .arg("sort_order", |a| {
            a.one_of(queries::SORT_ORDER)
                .description("asc (the default) or desc");
        });
        page_args(cmd);
        cmd.handler(list::<T>);
    });
}

/// A list command over `T`'s kind: the page its filter args select, its
/// entries under the kind's plural. A sort field the kind does not have is
/// `INVALID_INPUT` (the SDK refuses a value outside a closed filter's enum
/// or `asc`/`desc`, and a page arg that is not a count).
fn list<T: ListEntry>(call: &CommandCall<'_>) -> CommandOutput {
    let filter = match list_filter(call, T::KIND) {
        Ok(filter) => filter,
        Err(error) => return call.fail(&error, queries::INVALID_INPUT_EXIT),
    };
    let page = queries::list::<T>(call.graph(), &filter);
    call.render(&page.payload(T::KIND.plural), |out| {
        let rows: Vec<Vec<String>> = page.items.iter().map(ListEntry::row).collect();
        out.push_str(&table(T::HEADERS, &rows));
        if page.has_more {
            let _ = writeln!(
                out,
                "{} of {} {}; --offset {} for more",
                page.items.len(),
                page.total,
                T::KIND.plural,
                page.offset + page.items.len()
            );
        }
    })
}

/// The `ListFilter` `call`'s args set for `kind`, validated.
fn list_filter<'a>(
    call: &'a CommandCall<'_>,
    kind: &'a ListKind,
) -> Result<ListFilter<'a>, CommandError> {
    let mut filter = ListFilter::all(kind);
    (filter.offset, filter.limit) = page(call);
    for arg in kind.filters {
        if let Some(value) = call.str(arg.field()) {
            filter.equals.push((arg.field(), value));
        }
    }
    if let Some(tags) = call.str("tags") {
        filter.tags = tags
            .split(',')
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .collect();
    }
    if let Some(field) = call.str("sort_by") {
        if !queries::sortable(kind.kind, field) {
            return Err(queries::invalid_input(format!(
                "sort_by: a {} has no field '{field}'",
                kind.kind
            )));
        }
        filter.sort_by = field;
    }
    if let Some(order) = call.str("sort_order") {
        filter.descending = order == "desc";
    }
    Ok(filter)
}
