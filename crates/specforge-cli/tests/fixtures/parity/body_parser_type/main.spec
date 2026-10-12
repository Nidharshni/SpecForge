// A type body in the software extension's own syntax. The core grammar
// reads every line of it as a field, so its parse errors, if any, would be
// reported like any other.
type Money "Money" {
  amount   Decimal @min
  currency "USD"
  meta     string
}
