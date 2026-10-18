// A shopping cart: the demo project of docs/demo.md.

spec "shop" {
  version "0.1.0"
}

type cart "Shopping cart" {
  items string[]
  verify unit "starts empty"
}

behavior add_item "Add an item to the cart" {
  category   cart
  contract   "Adding a named item puts it in the cart once"
  types      [cart]
  invariants [no_duplicate_items]
  verify unit "adds the item"
  verify unit "rejects an empty name"
  verify unit "rejects a duplicate item"
}

invariant no_duplicate_items "An item is in the cart at most once" {
  guarantee "every item name appears at most once in cart.items"
  verify property "a second add of the same item fails"
}
