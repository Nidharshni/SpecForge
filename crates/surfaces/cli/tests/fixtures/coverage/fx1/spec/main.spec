behavior login "Log in" {
  contract "The system MUST let a known user log in"
  features [signin]
  verify unit "a known user logs in"
}

behavior logout "Log out" {
  contract "The system MUST end the session on logout"
}

behavior reset_password "Reset password" {
  contract "The system MUST expire reset links"
  verify unit "Reset link expires after one hour"
}

type Status = active | inactive

type Payload "Payload" {
  verify string @optional
  verify unit "Payload schema is valid"
}

feature signin "Sign in" {
  problem "Users cannot reach their data"
  solution "Let them log in"
}

property no_lost_login "A login is never lost" {
  property_type safety
  expression "once logged in, a session stays until logout"
  verify unit "no login is lost"
}
