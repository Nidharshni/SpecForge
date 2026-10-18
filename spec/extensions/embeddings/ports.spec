// Embeddings extension — outbound port for AI/ML embedding providers

use "types/errors"

port EmbeddingProvider "Embedding Provider" {
  direction outbound
  category  "ai/embeddings"
  method generateEmbeddings(texts: string[]) -> Result<f32[][], EmitterError>
  method computeSimilarity(query: string, candidates: string[]) -> Result<f32[], EmitterError>
  verify integration "EmbeddingProvider contract is satisfied"
}
