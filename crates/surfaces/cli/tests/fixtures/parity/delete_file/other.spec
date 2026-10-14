// The harness deletes this file and tells the LSP (plan 01, D4).
behavior wave "Wave" {
  category command
  contract "The system MUST wave"
  verify   unit "waves"
}
