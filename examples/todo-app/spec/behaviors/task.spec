// The operations the todo-app performs on tasks.

use "events/task"
use "features/task_management"
use "formal/properties"
use "invariants/task"
use "ports/task_repository"
use "types/task"

defaults behavior {
  ports    [TaskRepository]
  features [task_management]
}

behavior create_task "Create a Task" {
  category   command
  invariants [task_id_uniqueness]
  types      [Task, CreateTaskCommand, InvalidTitleError]
  produces   [task_created]
  contract   """
    When a CreateTaskCommand is received, the system MUST reject an empty or
    whitespace-only title with InvalidTitleError, otherwise it MUST persist a
    new Task with status `open` and a unique id, then emit a task_created event.
    It MUST return Result<Task, InvalidTitleError>.
  """
  verify unit "empty title is rejected"
  verify unit "valid title creates an open task"
  verify integration "created task is retrievable by id"
}

behavior complete_task "Complete a Task" {
  category   command
  invariants [completed_implies_timestamp]
  types      [Task, CompleteTaskCommand, TaskNotFoundError]
  produces   [task_completed]
  satisfies  [no_lost_completion]
  contract   """
    When a CompleteTaskCommand is received for an existing task, the system MUST
    set the task status to `done`, set completedAt to the current time, and emit
    a task_completed event. An unknown task id MUST return TaskNotFoundError.
    It MUST return Result<Task, TaskNotFoundError>.
  """
  verify unit "completing an unknown task returns TaskNotFoundError"
  verify unit "completing a task sets status to done"
  verify integration "task_completed event fires once per completion"
}

behavior list_tasks "List Tasks by Status" {
  category query
  types    [Task, TaskStatus]
  contract """
    When given a TaskStatus filter, the system MUST return all tasks with that
    status ordered by createdAt ascending. Listing cannot fail.
  """
  verify unit "listing open tasks excludes done tasks"
}
