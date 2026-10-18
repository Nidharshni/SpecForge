// Embeddings extension — types for entity embeddings and similarity search

type EntityEmbedding "Entity Embedding" {
  entity_id   string  @readonly
  /// Serialized as a JSON array of floats. The dimensionality depends on the configured embedding provider.
  vector      float[] @readonly
  source_text string  @readonly
  dimensions  integer @readonly
  verify unit "EntityEmbedding schema is valid"
}

type EmbeddingSimilarityConfig "Embedding Similarity Config" {
  top_k integer @optional // Max results for similarity search (default: 10)
  verify unit "EmbeddingSimilarityConfig schema is valid"
}

type SimilarityResult "Similarity Result" {
  entity_id string @readonly
  /// Range: 0.0 to 1.0 (cosine similarity)
  score     float  @readonly
  kind      string @readonly
  title     string @readonly
  verify unit "SimilarityResult schema is valid"
}
