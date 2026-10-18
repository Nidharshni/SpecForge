// Authoring features — typed edits of a project's entities

use "behaviors/authoring"

feature typed_edits "Typed Edits" {
  status     accepted
  priority   high
  problem    """
    Every change to a .spec file is a hand edit: SpecForge has no operation that
    creates an entity, sets or removes a field, adds or removes a reference, or
    deletes an entity. Only rename writes entity text, and the CLI has no rename.
    A person or an agent guesses the field order, the quoting and the alignment,
    and learns of a mistake from specforge check afterwards.
  """
  solution   """
    One operation plans a list of typed edits (create, set, unset, retitle, link,
    unlink, add or remove a verify obligation, delete) and writes them. Each
    value is written in the form its field's declaration decides and each
    touched block is laid out by the formatter. Nothing is written unless the
    edited project compiles with no error it did not already have (E080); the
    files are then written as one project change. The CLI gets new, set,
    unset, link, unlink and delete; MCP gets specforge.edit.
  """
  acceptance [
    "a value is written in the form its field declares, and a typed edit never produces W159 or W020",
    "every block an edit touches is format-clean afterwards, and no byte outside the touched blocks changes",
    "an edit that adds an error is refused with E080 and writes nothing",
    "a write is all or nothing across files",
    "an edit that changes nothing writes nothing",
    "edits apply in the order given, and a later edit sees what an earlier one created",
  ]
}
