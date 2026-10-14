behavior alpha "Alpha" {
  contract "The system MUST do alpha"
  verify unit "alpha works"
}

behavior beta "Beta" {
  contract "The system MUST do beta"
}

type Color = Red | Green

type Red "Red" {
}

type Green "Green" {
}
