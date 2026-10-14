use "types"

behavior greet "Greet" {
  category command
  types    [Greeting]
  contract "The system MUST greet the user"
  verify unit "greets the user"
}
