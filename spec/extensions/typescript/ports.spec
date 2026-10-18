// @specforge/typescript extension ports

use "extensions/typescript/types"
use "types/errors"

port TsSourceScanner "Ts Source Scanner" {
  direction outbound
  category  "source/typescript"
  method scanFile(path: string) -> Result<TsSourceItem[], EmitterError>
  method scanDirectory(root: string, config: TsExtensionConfig) -> Result<TsScanResult, EmitterError>
  method resolveBarrelExport(barrel_path: string, symbol_name: string) -> Result<TsSourceAnchor, EmitterError>
  method findDefinition(entity_id: string, project_root: string) -> Result<TsSourceAnchor, EmitterError>
  verify integration "TsSourceScanner contract is satisfied"
}

port TsProjectDetector "Ts Project Detector" {
  direction outbound
  category  "source/typescript"
  method detectMonorepo(root: string) -> Result<TsMonorepoInfo, EmitterError>
  method detectFrameworks(root: string) -> Result<TsFrameworkDetection[], EmitterError>
  method readPackageJson(path: string) -> Result<FieldMap, EmitterError>
  method readTsConfig(path: string) -> Result<FieldMap, EmitterError>
  verify integration "TsProjectDetector contract is satisfied"
}
