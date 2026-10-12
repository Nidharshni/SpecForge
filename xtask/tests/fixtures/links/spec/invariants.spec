invariant task_id_uniqueness "Task ID Uniqueness" {
  guarantee "No two tasks may share the same id."
  risk      medium
  verify unit "concurrent task creation never produces duplicate ids"
}
