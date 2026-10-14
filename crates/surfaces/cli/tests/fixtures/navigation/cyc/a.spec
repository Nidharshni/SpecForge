behavior alpha "A" {
  depends_on [beta]
}
behavior beta "B" {
  depends_on [alpha]
}
