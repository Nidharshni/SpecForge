module core_mod "Core" {
  description "The core"
  depends_on  [edge_mod]
}

module edge_mod "Edge" {
  description "The edge"
  depends_on  [core_mod]
}
