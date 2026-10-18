// @specforge/compliance extension field type definitions
//
// These types describe the fields declared in the extension manifest's
// entityKinds entries. They are not standalone AST types — they document
// the field schemas that the manifest registers into the FieldRegistry.

type ComplianceRegulation "Compliance Regulation" {
  jurisdiction   string
  effective_date string @optional
  controls       string[]
  verify unit "ComplianceRegulation schema is valid"
}

type ComplianceControl "Compliance Control" {
  category string
  maturity string
  evidence string[]
  verify unit "ComplianceControl schema is valid"
}

type ComplianceEvidence "Compliance Evidence" {
  evidence_type  string
  collected_date string @optional
  expires        string @optional
  verify unit "ComplianceEvidence schema is valid"
}

type ComplianceAudit "Compliance Audit" {
  audit_type string
  scope      string[]
  findings   string @optional
  verify unit "ComplianceAudit schema is valid"
}
