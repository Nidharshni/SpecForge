// No contract: the software extension's required-field rule reports it.
behavior greet "Greet" {
  category command
  verify   unit "greets the user"
}
