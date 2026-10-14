use "missing"

behavior greet "Greet" {
  category command
  contract "The system MUST greet the user"
  verify unit "greets the user"
}
