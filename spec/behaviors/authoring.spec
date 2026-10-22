// Authoring behaviors — typed edits of a project's entities

use "invariants/core"
use "ports/inbound"
use "ports/outbound"

defaults behavior {
  features [typed_edits]
  ports    [CompilerApi, FileSystem]
}

behavior create_entity "Create an Entity" {
  invariants [typed_edit_canonical_form, typed_edit_atomicity]
  category   mutation
  contract   """
    A create edit names a kind, an id, a title and a file relative to the spec
    root; every spelling of the file under the spec root is that file (its source key; ADR 0195). The block is written in the canonical form, after the last block of
    its kind in the file, or at the end of the file; a file that does not exist
    is created, unless the later edits of the list leave it empty: a file that does not
    exist reads as empty, and a file the list leaves as it read is not written. The id MUST be an identifier of 2 to 60 characters that is not a
    reserved word (E013), and MUST NOT be taken, the kind MUST be declared, and the file MUST be a project source.
    Each field is written as a set of it writes it, so a field that holds what its file's default holds, as the graph stores both,
    is not written, and each verify obligation as an add-verify writes it, so an equal one given twice is written
    once. The field flags of `specforge new` are the kind's fields in the project
    the command acts on. Its command line is one command read once with the kind's fields: an option may stand anywhere
    and one given twice is refused, --help with a kind is that kind's help and an unknown kind is refused as unknown_kind,
    and a line whose --path a field's value spells is refused; a field named like one of its options is no flag of it, and
    the kind's help says to set it with specforge set. A create takes whole fields; a `field.entry` path is refused.
    The skeleton of a new block of a kind is what a create of it writes with no values, with a slot where each value
    goes: the id, the title, each required field and the final cursor; a member's skeleton is its field written in its
    form (ADR 0191).
  """
  verify unit "new writes the block in the canonical form after the last block of its kind"
  verify unit "new creates a missing source file"
  verify unit "new refuses a file that is not a project source"
  verify unit "new reads every spelling of a file under the spec root as that file"
  verify unit "two spellings of one new file in one list are one file"
  verify unit "new refuses an id that is taken"
  verify unit "new refuses a reserved id before the edited project is compiled"
  verify unit "new compares what it writes with the default, not what it was given"
  verify unit "new never writes a field that restates its default"
  verify unit "new refuses a field.entry path"
  verify unit "new writes each field as set writes it"
  verify unit "new writes an equal verify obligation once"
  verify unit "a create's skeleton filled in is the block new writes"
  verify unit "every kind's skeleton formats to itself"
  verify unit "a member's skeleton is written in its field's form"
  verify unit "a file new creates is not written when the later edits leave it empty"
  verify integration "new reads its field flags from the project its --path names, wherever --path stands"
  verify integration "new --in reads a spelling of an existing file as that file"
  verify integration "new refuses a file named from the project root, naming its spec-root spelling"
  verify integration "new --in refuses a new file named from the project root, naming its spec-root spelling"
  verify integration "new refuses a field given twice that takes one value"
  verify integration "new refuses an unknown kind as unknown_kind"
  verify unit "a kind's field named like one of new's options is no flag of new, and the kind's help says to set it with set"
  verify integration "new --help begins with what new does"
  verify integration "new refuses one of its options given twice, wherever each stands"
  verify integration "new's help of a kind shows each of its options as every command declares it"
  verify integration "new --help of a kind the project does not declare is refused as unknown_kind"
  verify integration "new --help after a field flag prints the kind's help"
  verify integration "new refuses a command line whose --path a field's value spells"
}

behavior edit_entity_field "Edit an Entity Field" {
  invariants [typed_edit_canonical_form]
  category   mutation
  contract   """
    A set edit replaces a field's value, written in the form its declaration
    decides, a text that holds a line break, or whose one-line form passes the
    project's max_width, triple-quoted with its lines one indent of the
    project's format configuration past its key, and inserts an absent field
    at its rank (one-line values, then
    block fields, then triple-quoted text, in the kind's declared order). An
    unset edit leaves the field holding none: it removes the field and the comment lines directly above it, or, over
    a file default, writes `[]` for a list and `{}` for a block field; a field of one value with a default cannot
    hold none and is refused. A second unset changes nothing. A
    dotted `field.entry` path edits one entry of a block field the kind
    declares; its field is refused as any field is, a field that is not a declared block field, or a path whose
    parts are not each one identifier, is refused not_editable, and so is a path into a block field the entity
    writes as a value. A block field the block inherits from its
    file's defaults is copied into an own field before an entry of it is set or unset, and an own field that then
    holds what the default holds, as the graph stores both, is removed; a set of an entry of a block field the block writes as none, or not at all,
    writes the entry inside it, and annotations given to a set are written on the value it places, a new entry's
    included; a block field left with no entry over a default is written `{}`. An unset of an
    entry, or of a block field, that neither the block nor its file's defaults hold changes nothing. A retitle edit
    replaces the header title where the parse read it, or writes one after
    the name when the header has none; a union's string variant is never its
    title; a spec block's title is its name, and a retitle of it is
    refused not_editable. A field the kind does not declare is refused. A field that is the other
    side of a relation another kind owns is refused naming the owning field (ADR 0157).
    A set, unset, link or unlink of a field the block writes twice first removes its later entries, which the
    graph does not hold (E081, ADR 0215).
  """
  verify unit "set writes the value in the form its field declares"
  verify unit "a multi-line value is laid out with the project's format configuration"
  verify unit "set of an absent field inserts it at its rank"
  verify unit "unset removes the field and the comment above it"
  verify unit "unset leaves a field holding none, once"
  verify unit "set to a default's value leaves the block inheriting it, in any form"
  verify unit "set refuses a field the kind does not declare"
  verify unit "set refuses the other side of a relation naming its owning field"
  verify unit "set of a nested entry edits one line of the block field"
  verify unit "every verb that changes a field written twice leaves the block writing it once, holding what it says"
  verify unit "every verb reads a field.entry path the same way"
  verify unit "an entry of a block field written as a value is refused"
  verify unit "every verb leaves a field holding what it says, own or inherited"
  verify unit "set of an entry into a block field written as none writes it inside"
  verify unit "set of a new entry writes its annotations"
  verify unit "retitle replaces the header title"
  verify unit "retitle of a spec block is refused: its title is its name"
  verify unit "retitle of an untitled union writes the title after its name, never over a variant"
}

behavior edit_entity_link "Edit an Entity Reference List" {
  invariants [typed_edit_canonical_form]
  category   mutation
  contract   """
    A link edit adds a target to a list field and keeps the items as written;
    an unlink edit removes it, and removes the field when the last item goes.
    A link to a target already there changes nothing. A list written as one value is its one item. A field a block inherits
    from its file's defaults is copied into an own field before it changes, as
    the earlier edits of the list left the file, and an own field that holds what the default holds, as the graph stores both, is removed. A link or unlink
    takes a whole list field: a `field.entry` path names an entry of a block field, which holds one text, and is
    refused not_editable, naming set and unset. An unlink of a single reference
    the block inherits empties a field of one value over a default, which is
    refused as unset refuses it.
  """
  verify unit "link on the other side of a relation names the owning field of the target's kind"
  verify unit "a reference list's items are split at commas however they are given"
  verify unit "link appends the target and keeps the items as written"
  verify unit "unlink of the last item removes the field"
  verify unit "link of an inherited field copies the default"
  verify unit "an own field equal to the default is removed"
  verify unit "link of a target already there writes nothing"
  verify unit "a link after new starts from the file's default"
  verify unit "a link after unset starts from none"
  verify unit "link and unlink read a list written as one value as its one item"
  verify unit "link and unlink refuse a field.entry path as an entry of a block field"
  verify unit "unlink of an inherited single reference is refused as unset is"
  verify unit "a link and its unlink leave a block as it was"
}

behavior edit_entity_obligation "Edit an Entity Obligation" {
  invariants [typed_edit_canonical_form]
  category   mutation
  contract   """
    An add-verify edit inserts a verify line after the last verify statement,
    or before the closing brace. A remove-verify edit removes the statement
    with that kind and text. Where each statement is written is read from the
    block's parse: an entry named verify inside a block field is a field, never
    a statement (ADR 0187, amended). An equal line is not added twice.
  """
  verify unit "a verify line is added after the last one"
  verify unit "a verify line is removed by kind and text"
  verify unit "remove-verify removes the statement, never a block field's entry named verify"
  verify unit "add-verify writes after the last statement, never inside a block field"
  verify unit "add-verify on a union is refused as not_editable, a union has no verify statements"
}

behavior delete_entity "Delete an Entity" {
  invariants [typed_edit_atomicity]
  category   mutation
  contract   """
    A delete edit removes the entity's block and its doc comment, as the parse
    reads it: the // lines directly above the block with no blank line between.
    The format header is never a doc comment and stays. Deleting an entity that is still referenced leaves the project
    with an error, so it is refused with E080 unless the edit asks to remove
    every reference to the entity in the same change. The references are every
    token that names the entity, as the graph's one finder reads the texts the
    earlier edits left (ADR 0163). A token written where no typed edit writes,
    in a defaults block, in a field its kind does not declare or as a type name
    a derived field reads, refuses the delete as not_editable, naming where it
    is written.
  """
  verify unit "delete removes the block and the comment above it"
  verify unit "delete keeps the format header above the block"
  verify unit "delete of a referenced entity is E080"
  verify unit "delete with unlink_references removes every reference"
  verify unit "delete with unlink_references removes a scheme ref ID from a string list"
  verify unit "delete with unlink_references removes a reference list written as one value"
  verify unit "delete with unlink_references refuses a reference no typed edit writes"
  verify unit "delete with unlink_references refuses a type name a derived field reads"
}

behavior validate_typed_edit "Validate a Typed Edit" {
  invariants [typed_edit_canonical_form, typed_edit_atomicity]
  category   validation
  contract   """
    Edits are planned in order over the texts the project was compiled from.
    The edited project is compiled from what the project holds: the text of
    each source it read, the E025 of each it could not read, and the planned
    texts, reading no source again, so its verdict never depends on a file
    the project could not read; an edit that leaves an error-severity
    diagnostic the project did not have, with every check run (after an
    editor update that skipped the checks they are run for the comparison),
    is refused with E080 and writes nothing (an error is the same error after
    an edit moves it, even when its message names a position). Each edit declares what it may
    change in the graph: set and unset their field (a field.entry path's
    field), link and unlink their field, retitle the title, add-verify and
    remove-verify the obligations, create and delete their entity, a delete
    with unlink_references each referrer's field; an edge is declared by its
    source's declared field or by a created or deleted end. An edit whose
    edited graph changes anything else is refused as undeclared_change and
    writes nothing; E080 is judged first. Warnings and infos it adds are written and reported. A preview
    refuses exactly what the write would. The files are written as one project
    change: a file changed since the compile refuses the whole plan, and a
    failed write puts every file already written back. The report names the net
    change: the entities whose blocks the list, taken together, creates, changes or deletes, and the
    files whose text it changes; an entity a later edit puts back as it was is not named, and a
    block that only moves within its file is not either.
  """
  verify unit "a graph change no edit declares is beyond the declaration"
  verify unit "every verb's graph change is what it declares"
  verify unit "an edit that adds an error is E080 and writes nothing"
  verify unit "an error whose message names a position is the same error after an edit moves it"
  verify unit "an edit that adds a warning is written and reports it"
  verify unit "a preview refuses what the write would"
  verify unit "every block an edit touches is format-clean"
  verify unit "a failed write puts every file back"
  verify unit "an edit planned while the checks are skipped is compared with the checked report"
  verify unit "the report names what the edits change taken together"
  verify unit "a create links the names the project already writes for its ID"
}
