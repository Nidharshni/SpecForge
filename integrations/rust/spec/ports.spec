// specforge-test crate ports
// Boundaries between the crate and external systems.

use "types"

port ProcessExit "Process Exit" {
  direction outbound
  category  "system/lifecycle"
  method register_atexit(handler: fn()) -> Result<(), string>
  method get_binary_name() -> Result<string, string>
  requires {
    single_registration "atexit handler is registered at most once via std::sync::Once"
  }
}

port FileSystem "File System" {
  direction outbound
  category  "io/filesystem"
  method write_report(path: string, report: BinaryReport) -> Result<(), string>
  method file_exists(path: string) -> Result<boolean, string>
  requires {
    target_dir_writable "target/specforge/ directory is writable"
  }
}

port TestRegistry "Test Registry" {
  direction inbound
  category  "testing/registry"
  method record(entry: TestRecordEntry) -> Result<(), string>
  method drain() -> Result<TestRecordEntry[], string>
  requires {
    thread_safe "registry operations are safe under concurrent test execution"
  }
}
