// {project} — software specification
//
// Uses @specforge/software entity kinds: behavior, type, event, port, invariant.
//
// Try: specforge check

spec "{project}" {
  version "{version}"
}

type user "User account" {
  status draft
  verify "rejects an empty email"
}

behavior authenticate_user "Authenticate a user with credentials" {
  status   draft
  category auth
  contract "Given valid credentials, returns an auth token"
  produces [user_logged_in]

  verify "rejects invalid password"
  verify "returns token on success"
}

event user_logged_in "User successfully logged in" {
  payload user
  verify "is emitted once per successful login"
}
