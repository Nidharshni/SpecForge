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
    is created. The id MUST be an identifier of 2 to 60 characters and MUST NOT
    be taken, the kind MUST be declared, and the file MUST be a project source.
  """
  verify unit "new writes the block in the canonical form after the last block of its kind"
  verify unit "new creates a missing source file"
  verify unit "new refuses a file that is not a project source"
  verify unit "new refuses an id that is taken"
}

behavior edit_entity_field "Edit an Entity Field" {
  invariants [typed_edit_canonical_form]
  category   mutation
  contract   """
    A set edit replaces a field's value, written in the form its declaration
    decides, and inserts an absent field at its rank (one-line values, then
    block fields, then triple-quoted text, in the kind's declared order). An
    unset edit removes the field and the comment lines directly above it. A
    dotted field name edits one entry of a block field. A retitle edit
    replaces the header title. A field the kind does not declare is refused.
  """
  verify unit "set writes the value in the form its field declares"
  verify unit "set of an absent field inserts it at its rank"
  verify unit "unset removes the field and the comment above it"
  verify unit "set refuses a field the kind does not declare"
  verify unit "set of a nested entry edits one line of the block field"
  verify unit "retitle replaces the header title"
}

behavior edit_entity_link "Edit an Entity Reference List" {
  invariants [typed_edit_canonical_form]
  category   mutation
  contract   """
    A link edit adds a target to a list field and keeps the items as written;
    an unlink edit removes it, and removes the field when the last item goes.
    A link to a target already there changes nothing. A field a block inherits
    from its file's defaults is copied into an own field before it changes, and
    an own field equal to the default is removed.
  """
  verify unit "link appends the target and keeps the items as written"
  verify unit "unlink of the last item removes the field"
  verify unit "link of an inherited field copies the default"
  verify unit "an own field equal to the default is removed"
  verify unit "link of a target already there writes nothing"
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
    every reference to the entity in the same change.
  """
  verify unit "delete removes the block and the comment above it"
  verify unit "delete of a referenced entity is E080"
  verify unit "delete with unlink_references removes every reference"
}

behavior validate_typed_edit "Validate a Typed Edit" {
  invariants [typed_edit_canonical_form, typed_edit_atomicity]
  category   validation
  contract   """
    Edits are planned in order over the texts the project was compiled from.
    The edited project is compiled; an edit that leaves an error-severity
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
