type Item "Item" {
  name string
}

port store "Store" {
  direction outbound
  method save(item: Item) -> Item
}
