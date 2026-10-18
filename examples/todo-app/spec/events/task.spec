// Domain events emitted by task behaviors.

use "types/task"

event task_created "Task Created" {
  channel  "tasks.created"
  payload  TaskCreatedPayload
  category domain
  verify integration "task_created is emitted after a successful create"
}

event task_completed "Task Completed" {
  channel  "tasks.completed"
  payload  TaskCompletedPayload
  category domain
  verify integration "task_completed is emitted after a successful complete"
}
