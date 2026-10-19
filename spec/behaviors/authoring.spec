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
    root. The block is written in the canonical form, after the last block of
    its kind in the file, or at the end of the file; a file that does not exist
    is created. The id MUST be an identifier of 2 to 60 characters that is not a
    reserved word (E013), and MUST NOT be taken, the kind MUST be declared, and the file MUST be a project source.
    A field whose written value restates its file's default for the kind (W161)
    is not written: the block inherits it. The field flags of `specforge new` are the kind's fields in the project
    the command acts on.
  """
  verify unit "new writes the block in the canonical form after the last block of its kind"
  verify unit "new creates a missing source file"
  verify unit "new refuses a file that is not a project source"
  verify unit "new refuses an id that is taken"
  verify unit "new refuses a reserved id before the edited project is compiled"
  verify unit "new compares what it writes with the default, not what it was given"
  verify unit "new never writes a field that restates its default"
  verify integration "new reads its field flags from the project its --path names, wherever --path stands"
  verify integration "new refuses a field given twice that takes one value"
  verify integration "new refuses an unknown kind as unknown_kind"
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
    unset edit removes the field and the comment lines directly above it. A
    dotted field name edits one entry of a block field. A retitle edit
    replaces the header title. A field the kind does not declare is refused. A field that is the other
    side of a relation another kind owns is refused naming the owning field (ADR 0157).
  """
  verify unit "set writes the value in the form its field declares"
  verify unit "a multi-line value is laid out with the project's format configuration"
  verify unit "set of an absent field inserts it at its rank"
  verify unit "unset removes the field and the comment above it"
  verify unit "set refuses a field the kind does not declare"
  verify unit "set refuses the other side of a relation naming its owning field"
  verify unit "set of a nested entry edits one line of the block field"
  verify unit "retitle replaces the header title"
}

behavior edit_entity_link "Edit an Entity Reference List" {
  invariants [typed_edit_canonical_form]
  category   mutation
  contract   """
    A link edit adds a target to a list field and keeps the items as written;
    an unlink edit removes it, and removes the field when the last item goes.
    A link to a target already there changes nothing. A list written as one value is its one item. A field a block inherits
    from its file's defaults is copied into an own field before it changes, as
    the earlier edits of the list left the file, and an own field equal to the default is removed.
  """
  verify unit "link on the other side of a relation names the owning field of the target's kind"
  verify unit "a reference list's items are split at commas however they are given"
  verify unit "link appends the target and keeps the items as written"
  verify unit "unlink of the last item removes the field"
  verify unit "link of an inherited field copies the default"
  verify unit "an own field equal to the default is removed"
  verify unit "link of a target already there writes nothing"
  verify unit "a link after new starts from the file's default"
  verify unit "a link after unset starts from the file's default"
  verify unit "link and unlink read a list written as one value as its one item"
}

behavior edit_entity_obligation "Edit an Entity Obligation" {
  invariants [typed_edit_canonical_form]
  category   mutation
  contract   """
    An add-verify edit inserts a verify line after the last one, or before the
    closing brace. A remove-verify edit removes the line with that kind and
    text. An equal line is not added twice.
  """
  verify unit "a verify line is added after the last one"
  verify unit "a verify line is removed by kind and text"
}

behavior delete_entity "Delete an Entity" {
  invariants [typed_edit_atomicity]
  category   mutation
  contract   """
    A delete edit removes the entity's block and the comment lines directly
    above it. Deleting an entity that is still referenced leaves the project
    with an error, so it is refused with E080 unless the edit asks to remove
    every reference to the entity in the same change. The references are those
    the graph build links, found by the rule that decides what a value
    references (ADR 0093) in the texts the earlier edits left. A reference
    written where no typed edit writes, in a defaults block or in a field its
    kind does not declare, refuses the delete as not_editable, naming where it
    is written.
  """
  verify unit "delete removes the block and the comment above it"
  verify unit "delete of a referenced entity is E080"
  verify unit "delete with unlink_references removes every reference"
  verify unit "delete with unlink_references removes a scheme ref ID from a string list"
  verify unit "delete with unlink_references removes a reference list written as one value"
  verify unit "delete with unlink_references refuses a reference no typed edit writes"
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
    diagnostic the project did not have is refused with E080 and writes
    nothing. Warnings and infos it adds are written and reported. A preview
    refuses exactly what the write would. The files are written as one project
    change: a file changed since the compile refuses the whole plan, and a
    failed write puts every file already written back.
  """
  verify unit "an edit that adds an error is E080 and writes nothing"
  verify unit "an edit that adds a warning is written and reports it"
  verify unit "a preview refuses what the write would"
  verify unit "every block an edit touches is format-clean"
  verify unit "a failed write puts every file back"
}
