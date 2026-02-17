# RAPTOR Architecture Diagram

## System Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        GenHat RAG Pipeline                       │
│                                                                   │
│  Phase 1: Instant     Phase 2: Background    Phase 3: RAPTOR    │
│  ┌──────────────┐     ┌──────────────┐      ┌──────────────┐   │
│  │   Ingest     │────▶│   Enrich     │─────▶│ Build Tree   │   │
│  │   Document   │     │   Chunks     │      │  (On-Demand) │   │
│  └──────────────┘     └──────────────┘      └──────────────┘   │
│         │                    │                      │            │
│         ▼                    ▼                      ▼            │
│  ┌──────────────┐     ┌──────────────┐      ┌──────────────┐   │
│  │  SQLite DB   │     │  Enriched    │      │ RAPTOR Tree  │   │
│  │  + BM25 Index│     │  Embeddings  │      │   Storage    │   │
│  └──────────────┘     └──────────────┘      └──────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

## RAPTOR Tree Structure

```
Document: "Research Paper on Machine Learning" (100 chunks)
├─ Level 2 (Root)
│  └─ Node 1 (ID: 301)
│     Summary: "Paper discusses ML techniques..."
│     Confidence: -0.8 (HIGH → use summary)
│     Children: [201, 202, 203]
│
├─ Level 1 (Cluster Summaries)
│  ├─ Node 201
│  │  Summary: "Introduction and background on neural networks..."
│  │  Confidence: -1.2 (MEDIUM → use summary)
│  │  Children: [1, 2, 3, 4, 5]
│  │
│  ├─ Node 202
│  │  Summary: "The document discusses various approaches..."
│  │  Confidence: -2.1 (LOW → expand to children)
│  │  Children: [6, 7, 8, 9]
│  │
│  └─ Node 203
│     Summary: "Experimental results show significant improvements..."
│     Confidence: -0.6 (HIGH → use summary)
│     Children: [10, 11, 12, 13, 14]
│
└─ Level 0 (Raw Chunks)
   ├─ Chunk 1: "Machine learning has revolutionized..."
   ├─ Chunk 2: "Deep learning architectures consist of..."
   ├─ Chunk 3: "Training neural networks requires..."
   └─ ... (97 more chunks)
```

## Tree Building Process

```
Input: Document with 50 chunks

┌─────────────────────────────────────────────────────────────┐
│ STEP 1: Collect Embeddings                                  │
├─────────────────────────────────────────────────────────────┤
│ • Get all 50 chunk embeddings from database                 │
│ • Prefer enriched embeddings if available                   │
│ • Result: [(chunk_id_1, embedding_1), ..., (50, emb_50)]  │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ STEP 2: Level 1 - Cluster Chunks                           │
├─────────────────────────────────────────────────────────────┤
│ • Determine k = 50 / 3 = 16 (too many)                     │
│ • Cap at MAX_CLUSTERS_PER_LEVEL = 10                       │
│ • Run k-means with k=10                                     │
│ • Result: 10 clusters of 5 chunks each                     │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ STEP 3: Summarize Each Cluster                             │
├─────────────────────────────────────────────────────────────┤
│ For cluster 1 (chunks 1,5,12,23,45):                       │
│   • Concatenate chunk texts                                 │
│   • LLM: "Summarize: <texts>"                              │
│   • Extract confidence: -1.2                                │
│   • Embed summary: [0.12, -0.45, ...]                     │
│   • Store as Node 101 with children=[1,5,12,23,45]        │
│                                                             │
│ Repeat for all 10 clusters → 10 RAPTOR nodes              │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ STEP 4: Level 2 - Cluster Summaries                        │
├─────────────────────────────────────────────────────────────┤
│ • Input: 10 RAPTOR nodes from Level 1                      │
│ • Determine k = 10 / 3 = 3                                 │
│ • Run k-means with k=3                                      │
│ • Result: 3 clusters                                        │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ STEP 5: Create Root Summaries                              │
├─────────────────────────────────────────────────────────────┤
│ For cluster 1 (nodes 101,102,103,104):                    │
│   • Concatenate node summary texts                          │
│   • LLM: "Summarize: <summaries>"                         │
│   • Extract confidence: -0.8                                │
│   • Embed summary                                           │
│   • Store as Node 201 with children=[101,102,103,104]     │
│                                                             │
│ Repeat → 3 root nodes                                      │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
    COMPLETE
    Tree: 13 nodes (10 L1 + 3 L2)
```

## Retrieval Process

```
Query: "What are the main findings?"

┌─────────────────────────────────────────────────────────────┐
│ STEP 1: Embed Query                                         │
├─────────────────────────────────────────────────────────────┤
│ • query_embedding = embed("What are the main findings?")   │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ STEP 2: Vector Search Over RAPTOR Nodes                    │
├─────────────────────────────────────────────────────────────┤
│ • Compare query_embedding to all 13 RAPTOR node embeddings │
│ • Compute cosine similarity for each                        │
│ • Sort by similarity descending                             │
│ • Top 5 results:                                            │
│   1. Node 203 (L1) - similarity: 0.87                      │
│   2. Node 201 (L2) - similarity: 0.82                      │
│   3. Node 105 (L1) - similarity: 0.79                      │
│   4. Node 202 (L1) - similarity: 0.75                      │
│   5. Node 108 (L1) - similarity: 0.71                      │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ STEP 3: Confidence-Aware Expansion                         │
├─────────────────────────────────────────────────────────────┤
│ Node 203 (conf: -0.6, threshold: -1.5)                    │
│   ✓ -0.6 > -1.5 → HIGH confidence                         │
│   → Use summary: "Experimental results show..."            │
│                                                             │
│ Node 201 (conf: -0.8, threshold: -1.5)                    │
│   ✓ -0.8 > -1.5 → HIGH confidence                         │
│   → Use summary: "Paper discusses ML techniques..."        │
│                                                             │
│ Node 105 (conf: -1.2, threshold: -1.5)                    │
│   ✓ -1.2 > -1.5 → MEDIUM confidence                       │
│   → Use summary: "Training methodology includes..."        │
│                                                             │
│ Node 202 (conf: -2.1, threshold: -1.5)                    │
│   ✗ -2.1 < -1.5 → LOW confidence                          │
│   → EXPAND to children: [6, 7, 8, 9]                      │
│   → Fetch chunk texts: chunk 6, 7, 8, 9                   │
│                                                             │
│ Node 108 (conf: -1.8, threshold: -1.5)                    │
│   ✗ -1.8 < -1.5 → LOW confidence                          │
│   → EXPAND to children: [15, 16, 17]                      │
│   → Fetch chunk texts: chunk 15, 16, 17                   │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
┌─────────────────────────────────────────────────────────────┐
│ STEP 4: Build Context & Generate Answer                    │
├─────────────────────────────────────────────────────────────┤
│ Context:                                                     │
│ [Source 1 - Node 203 Summary]                              │
│ Experimental results show significant improvements...       │
│                                                             │
│ [Source 2 - Node 201 Summary]                              │
│ Paper discusses ML techniques including...                  │
│                                                             │
│ [Source 3 - Node 105 Summary]                              │
│ Training methodology includes gradient descent...           │
│                                                             │
│ [Source 4 - Chunk 6]                                        │
│ The proposed method achieved 95% accuracy...                │
│                                                             │
│ [Source 5 - Chunk 7]                                        │
│ Compared to baseline approaches, our system...              │
│                                                             │
│ ... (more expanded chunks)                                  │
│                                                             │
│ → LLM generates answer using this context                   │
└─────────────────────────────────────────────────────────────┘
         │
         ▼
    RESULT
    Answer + Sources
```

## Database Schema

```sql
┌────────────────────────────────────────────────────────┐
│ TABLE: raptor_nodes                                    │
├────────────────────────────────────────────────────────┤
│ id                 INTEGER PRIMARY KEY                 │
│ doc_id             INTEGER → documents(id)             │
│ level              INTEGER (0=chunks, 1=L1, 2=L2...)  │
│ parent_id          INTEGER → raptor_nodes(id) or NULL │
│ summary_text       TEXT (LLM-generated summary)        │
│ confidence_score   REAL (from logprobs or heuristic)   │
│ child_ids          TEXT (JSON array of child IDs)      │
│ embedding          BLOB (f32 array, serialized)        │
│ created_at         TEXT (timestamp)                    │
└────────────────────────────────────────────────────────┘

Example Rows:

┌────┬────────┬───────┬───────────┬─────────────────────┬────────┬─────────────┐
│ id │ doc_id │ level │ parent_id │ summary_text        │ conf   │ child_ids   │
├────┼────────┼───────┼───────────┼─────────────────────┼────────┼─────────────┤
│101 │   1    │   1   │   NULL    │ "Intro to ML..."    │ -1.2   │ [1,2,3,4]  │
│102 │   1    │   1   │   NULL    │ "Methods used..."   │ -2.1   │ [5,6,7]    │
│201 │   1    │   2   │   NULL    │ "Paper overview..." │ -0.8   │ [101,102]  │
└────┴────────┴───────┴───────────┴─────────────────────┴────────┴─────────────┘
```

## Confidence Scoring

```
┌─────────────────────────────────────────────────────────────┐
│ Confidence Score (currently heuristic-based)                │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  -2.5 ├─────────────────────────────────────────────┤      │
│       │ Generic/Short                                │      │
│       │ "The document discusses..."                  │      │
│  -2.0 ├─────────────────────────────────────────────┤      │
│       │                                               │      │
│  -1.5 ├═════════════════════════════════════════════┤ ← Threshold
│       │ Medium Detail                                │      │
│       │ "The research focuses on neural networks..." │      │
│  -1.0 ├─────────────────────────────────────────────┤      │
│       │                                               │      │
│  -0.5 ├─────────────────────────────────────────────┤      │
│       │ Detailed/Specific                            │      │
│       │ "Experimental results demonstrate that..."   │      │
│   0.0 ├─────────────────────────────────────────────┤      │
│                                                             │
│ Higher confidence → Use summary directly                    │
│ Lower confidence → Expand to children                       │
└─────────────────────────────────────────────────────────────┘
```

## API Flow

```
Frontend (JavaScript)              Backend (Rust)
─────────────────────              ──────────────

User clicks "Build Tree"
    │
    ├─ invoke('build_raptor_tree')
    │                               ├─ build_raptor_tree()
    │                               │   ├─ Get chunk embeddings
    │                               │   ├─ K-means clustering
    │                               │   ├─ LLM summarization
    │                               │   └─ Store RAPTOR nodes
    │                               │
    ◀─ Return status               ─┘
    │  {nodes_created: 13, levels: 2}
    │
    ├─ Show success message
    └─ Enable "Query" button


User enters query & clicks search
    │
    ├─ invoke('query_rag_with_raptor')
    │                               ├─ query_with_raptor()
    │                               │   ├─ Check if tree exists
    │                               │   ├─ raptor_retrieve()
    │                               │   │   ├─ Embed query
    │                               │   │   ├─ Vector search
    │                               │   │   └─ Expand nodes
    │                               │   ├─ Build context
    │                               │   └─ LLM generate answer
    │                               │
    ◀─ Return result               ─┘
    │  {answer: "...", sources: [...]}
    │
    ├─ Display answer
    └─ Show source citations
```

## Performance Comparison

```
Standard RAG vs RAPTOR

┌────────────────────────────────────────────────────────────┐
│ Scenario: 100-chunk document, "Summarize main points"     │
├────────────────────────────────────────────────────────────┤
│                                                            │
│ Standard RAG:                                              │
│   • Vector search: 100 chunk embeddings                   │
│   • Top-5 chunks retrieved                                │
│   • Context length: ~2000 tokens                          │
│   • Pros: Fast, precise citations                         │
│   • Cons: May miss high-level patterns                    │
│                                                            │
│ RAPTOR:                                                    │
│   • Vector search: ~30 RAPTOR node embeddings             │
│   • Top-5 nodes retrieved (mix of summaries & chunks)     │
│   • Context length: ~1500 tokens (summaries are concise)  │
│   • Pros: Better for summaries, multi-topic docs          │
│   • Cons: One-time tree building overhead                 │
│                                                            │
└────────────────────────────────────────────────────────────┘

Tree Building Cost (one-time):
  • 100 chunks → 30 nodes
  • 30 LLM calls × 2s each = 60 seconds
  • Amortized over many queries

Retrieval Speed:
  • RAPTOR: 30 comparisons
  • Standard: 100 comparisons
  • RAPTOR is 3x faster for vector search
```

This diagram-based documentation provides a visual understanding of how RAPTOR works in the GenHat system!
