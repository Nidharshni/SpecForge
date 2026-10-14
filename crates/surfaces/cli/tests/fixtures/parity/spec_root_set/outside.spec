// Outside spec_root: no surface may read this file.
behavior greet "Duplicate outside the spec root" {
  category command
  contract "The system MUST not be read"
}
