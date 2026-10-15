// Logical Data Model types — intermediate representation for schema visualization

type ModelFormat = "markdown" | "mermaid" | "dot" | "json" | "dbml"

type GroupBy = "extension" | "none"

type FieldLevel = "none" | "keys" | "all"

type Cardinality = "1:1" | "1:N" | "N:1" | "N:M"

type ModelOptions {
  format    ModelFormat @readonly
  group_by  GroupBy     @readonly
  fields    FieldLevel  @readonly
  extension string      @readonly @optional
  kinds     string[]    @readonly
  root      ModelRoot   @readonly @optional
  verify unit "ModelOptions schema is valid"
}

type ModelRoot "The kinds within depth hops of a root kind" {
  kind  string  @readonly
  depth integer @readonly @optional
  verify unit "ModelRoot schema is valid"
}

type ModelField {
  name           string    @readonly
  field_type     FieldType @readonly
  required       boolean   @readonly
  description    string    @readonly @optional
  default_value  string    @readonly @optional
  enum_values    string[]  @readonly @optional
  is_primary_key boolean   @readonly
  references     string    @readonly @optional // target entity name
  contributed_by string    @readonly @optional // the extension an entity enhancement's field comes from
  verify unit "ModelField schema is valid"
}

type ModelEntity {
  name        string       @readonly
  extension   string       @readonly
  description string       @readonly @optional
  fields      ModelField[] @readonly
  verify unit "ModelEntity schema is valid"
}

type ModelRelationship {
  name         string      @readonly
  source       string      @readonly
  target       string      @readonly
  cardinality  Cardinality @readonly
  source_field string      @readonly @optional
  description  string      @readonly @optional
  verify unit "ModelRelationship schema is valid"
}

type ModelExtension {
  name         string  @readonly
  version      string  @readonly
  entity_count integer @readonly
  // the edge types it declares that name a drawn relationship
  edge_count   integer @readonly
  verify unit "ModelExtension schema is valid"
}

type ModelIntermediate "ERD-oriented intermediate representation" {
  schema_version string              @readonly // the drawn schema's version
  extensions     ModelExtension[]    @readonly
  entities       ModelEntity[]       @readonly
  relationships  ModelRelationship[] @readonly
  verify unit "ModelIntermediate schema is valid"
}
