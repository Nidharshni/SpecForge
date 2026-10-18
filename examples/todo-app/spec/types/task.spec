// Data shapes for the todo-app domain.

type Task "A unit of work the user wants to track" {
  id          string    @readonly @unique
  title       string
  description string    @optional
  status      TaskStatus
  createdAt   timestamp @readonly
  completedAt timestamp @optional
  verify unit "completedAt is set exactly when status is done"
}

type TaskStatus "A discriminated union of the states a task can be in." = open
  | in_progress
  | done
  | archived

type CreateTaskCommand "Command payload for creating a task." {
  title       string
  description string @optional
  verify unit "rejects a blank title"
}

type CompleteTaskCommand "Command payload for completing a task." {
  taskId string
  verify unit "rejects an empty taskId"
}

type TaskNotFoundError "Error returned when a task id does not resolve." {
  _tag    "TaskNotFoundError" @literal
  taskId  string
  message string
  verify unit "message names the missing taskId"
}

type InvalidTitleError "Error returned when a title fails validation." {
  _tag   "InvalidTitleError" @literal
  reason string
  verify unit "reason says why the title was rejected"
}

type TaskCreatedPayload "Event payload carried when a task is created." {
  taskId string @readonly
  title  string
  verify unit "round-trips through JSON unchanged"
}

type TaskCompletedPayload "Event payload carried when a task is completed." {
  taskId      string @readonly
  completedAt timestamp
  verify unit "round-trips through JSON unchanged"
}
