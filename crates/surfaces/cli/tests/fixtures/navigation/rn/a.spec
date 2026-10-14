invariant session_limit "session_limit cap" {
  guarantee "session_limit is never exceeded"
}
behavior login "Login" {
  invariants [session_limit]
  // keeps session_limit
  verify unit "login respects session_limit"
}
