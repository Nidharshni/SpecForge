import { Slide, Heading, Text, FlexBox, Notes } from 'spectacle';
import type { JSX } from 'react';
import { colors } from '../theme/colors';
import { FullBleedCode } from '../components/layout/FullBleedCode';
import { compilerErrorsExample, graphProtocolJsonExample, traceabilityExample } from '../data/code-snippets';

export const theCompilerSlides: JSX.Element[] = [
  // ─── Slide 29: The Compiler Pipeline ──────────────────────────────────
  <Slide key="compiler-pipeline" backgroundColor={colors.bg.primary}>
    <FlexBox flexDirection="column" alignItems="center" justifyContent="center" height="100%">
      <Text fontSize="15px" color={colors.accent.teal} margin="0 0 12px 0" style={{ textTransform: 'uppercase', letterSpacing: '0.15em', fontWeight: 600 }}>The Compiler</Text>
      <Heading fontSize="44px" color={colors.text.primary} margin="0 0 32px 0" style={{ textAlign: 'center' }}>
        4-Stage Rust Pipeline
      </Heading>

      <div style={{ display: 'flex', gap: 18, maxWidth: 920, width: '100%', marginBottom: 28 }}>
        {[
          { step: '1', title: 'Parse', desc: 'Tree-sitter grammar accepts ANY `keyword name { fields }` block. Error recovery: one malformed block doesn\'t kill the file. String interning via lasso crate for O(1) ID comparison. Output: untyped AST.', color: colors.accent.teal },
          { step: '2', title: 'Resolve', desc: 'Extensions register entity kinds via ManifestV2. Resolver walks use imports, links every reference to its definition, builds a petgraph directed graph with typed nodes and edges. Topological sort for dependency ordering.', color: colors.brand },
          { step: '3', title: 'Validate', desc: '51 diagnostic rules, ALL from extensions. Categories: unresolved_reference (E001), no_incoming_edges (unreferenced invariants W003, unused ports W005, orphan events W007), cycle_detection (E007), field_value_constraint, custom (Wasm-backed validators). Rich errors via ariadne with file:line:col and "did you mean?" suggestions.', color: colors.accent.red },
          { step: '4', title: 'Export', desc: 'Graph Protocol JSON. Three formats: --format=graph (full entity graph), --format=context (agent-optimized with inlined relationships), --format=brief (minimal metadata). --scope=entity_id extracts subgraphs. --token-budget=N controls output size.', color: colors.accent.green },
        ].map((item) => (
          <div key={item.step} style={{ flex: 1, padding: '22px 18px', background: 'rgba(255,255,255,0.03)', border: '1px solid rgba(255,255,255,0.06)', borderTop: `4px solid ${item.color}`, borderRadius: '0 0 14px 14px' }}>
            <div style={{ display: 'flex', alignItems: 'center', gap: 12, marginBottom: 14 }}>
              <div style={{ width: 32, height: 32, borderRadius: '50%', background: `${item.color}20`, display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 16, fontWeight: 800, color: item.color, flexShrink: 0 }}>{item.step}</div>
              <Text fontSize="20px" color={colors.text.primary} margin="0" fontWeight="bold">{item.title}</Text>
            </div>
            <Text fontSize="14px" color={colors.text.secondary} margin="0" style={{ lineHeight: 1.65 }}>{item.desc}</Text>
          </div>
        ))}
      </div>

      <div style={{ display: 'flex', gap: 16, maxWidth: 920, width: '100%' }}>
        {[
          { label: 'Tree-sitter', desc: 'Incremental parsing, error recovery', color: colors.accent.teal },
          { label: 'petgraph', desc: 'Typed entity graph, cycle detection', color: colors.brand },
          { label: 'ariadne', desc: 'Rich error diagnostics with source spans', color: colors.accent.red },
          { label: 'Extism', desc: 'Wasm extension runtime, AOT cached', color: colors.accent.purple },
        ].map((item) => (
          <div key={item.label} style={{ flex: 1, padding: '10px 14px', background: `${item.color}08`, borderLeft: `3px solid ${item.color}`, borderRadius: '0 8px 8px 0' }}>
            <Text fontSize="14px" color={item.color} margin="0" fontWeight="bold" style={{ fontFamily: '"JetBrains Mono", monospace' }}>{item.label}</Text>
            <Text fontSize="14px" color={colors.text.muted} margin="2px 0 0 0">{item.desc}</Text>
          </div>
        ))}
      </div>
    </FlexBox>
    <Notes>4-stage pipeline: Parse → Resolve → Validate → Export. All in Rust. Tree-sitter for parsing, petgraph for the graph, ariadne for beautiful errors, Extism for Wasm extensions. Incremental compilation for LSP/watch mode. Less than 100ms for file-change-to-diagnostics on 500 files.</Notes>
  </Slide>,

  // ─── Slide 30: What the Compiler Catches ──────────────────────────────
  <FullBleedCode
    key="compiler-errors"
    filename="$ specforge check --path spec/"
    language="rust"
    badge="compiler errors"
    badgeColor={colors.accent.red}
    code={compilerErrorsExample}
    takeaway={'E001: unresolved references with fuzzy "did you mean?" suggestions. E006: events with no producer behavior. W003: unreferenced invariants. W005: unreferenced ports. W007: orphan events. Every diagnostic has file:line:col, severity, and a help message.'}
    notes={<Notes>THIS is the proof. E001 catches typos with suggestions. E006 catches events no behavior produces. W003 catches invariants not enforced by any behavior. W005 catches dead ports. W007 catches orphan events. Every error has file:line:col.</Notes>}
  />,

  // ─── Slide 31: Graph Protocol JSON Output ─────────────────────────────
  <FullBleedCode
    key="graph-protocol-json"
    filename="$ specforge export --format=context --scope=rate_limited_auth"
    language="json"
    badge="graph protocol"
    badgeColor={colors.accent.green}
    code={graphProtocolJsonExample}
    takeaway={'The Graph Protocol is the actual product — not the compiler. Typed entities with explicit kinds. Typed edges with source, target, and relationship label. Deterministic ordering. 2,400 tokens from 8 entities and 7 edges — down from 45,000 tokens of raw source. Any tool that produces this JSON is a valid SpecForge compiler.'}
    notes={<Notes>THE output. The Graph Protocol JSON. Every entity has a kind, every relationship is typed, every constraint is explicit. 19x compression from raw source. An agent reading this KNOWS the token expiry, KNOWS the rate limiter, KNOWS what events to emit.</Notes>}
  />,

  // ─── Slide 32: Traceability ───────────────────────────────────────────
  <FullBleedCode
    key="traceability"
    filename="$ specforge trace create_user"
    language="bash"
    badge="traceability"
    badgeColor={colors.accent.blue}
    code={traceabilityExample}
    takeaway={'Three traceability layers: Intent (verify declarations + file-reference fields), Linkage (tests field mapping to test files), Proof (specforge-report.json with PASS/FAIL results). Coverage is computed from the graph, not guessed.'}
    notes={<Notes>This is the traceability loop. specforge trace shows the full chain: feature → behavior → invariants → ports → events → verify declarations → test files → PASS/FAIL results. Coverage is computed, not guessed. This makes agents self-correcting.</Notes>}
  />,

  // ─── Slide 33: Before/After ───────────────────────────────────────────
  <Slide key="before-after" backgroundColor={colors.bg.primary}>
    <FlexBox flexDirection="column" alignItems="center" justifyContent="flex-start" height="100%" style={{ paddingTop: 14 }}>
      <Heading fontSize="38px" color={colors.text.primary} margin="0 0 6px 0" style={{ textAlign: 'center' }}>
        Same Model, Same Prompt — <span style={{ color: colors.accent.teal }}>Different Context</span>
      </Heading>

      {/* Accuracy metric banner */}
      <div style={{ display: 'flex', alignItems: 'center', gap: 20, marginBottom: 14, padding: '10px 28px', background: `${colors.brand}08`, border: `2px solid ${colors.brand}`, borderRadius: 12 }}>
        <div style={{ textAlign: 'center' }}>
          <Text fontSize="28px" color={colors.accent.red} margin="0" fontWeight="bold">~30%</Text>
          <Text fontSize="14px" color={colors.text.muted} margin="2px 0 0 0">prose context</Text>
        </div>
        <Text fontSize="28px" color={colors.brand} margin="0">→</Text>
        <div style={{ textAlign: 'center' }}>
          <Text fontSize="28px" color={colors.accent.green} margin="0" fontWeight="bold">70-85%</Text>
          <Text fontSize="14px" color={colors.text.muted} margin="2px 0 0 0">graph context</Text>
        </div>
        <div style={{ borderLeft: `2px solid ${colors.brand}40`, paddingLeft: 20 }}>
          <Text fontSize="16px" color={colors.brand} margin="0" fontWeight="bold">First-attempt accuracy (target)</Text>
        </div>
      </div>

      <div style={{ display: 'flex', gap: 20, maxWidth: 940, width: '100%', flex: 1, minHeight: 0 }}>
        {/* WITHOUT */}
        <div style={{ flex: 1, background: colors.bg.card, borderRadius: 14, border: '1px solid rgba(255,255,255,0.06)', borderTop: `3px solid ${colors.accent.red}`, padding: '14px 18px', display: 'flex', flexDirection: 'column', gap: 10, overflow: 'hidden' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <Text fontSize="18px" color={colors.accent.red} margin="0" fontWeight="bold">Without SpecForge</Text>
            <Text fontSize="14px" color={colors.text.muted} margin="0">45K tokens</Text>
          </div>
          <div style={{ flex: 1, background: '#0a0a12', borderRadius: 10, padding: '14px 16px', fontFamily: '"JetBrains Mono", monospace', fontSize: 15, lineHeight: 1.55, color: colors.text.secondary, overflow: 'hidden', whiteSpace: 'pre' }}>
            <div style={{ color: colors.text.muted }}>// Agent guesses from source files</div>
            <div>&nbsp;</div>
            <div><span style={{ color: colors.accent.purple }}>async</span> <span style={{ color: colors.accent.blue }}>authenticate</span>(req) {'{'}</div>
            <div style={{ color: colors.accent.red }}>{'  '}// Forgot rate limiter entirely</div>
            <div>{'  '}<span style={{ color: colors.accent.purple }}>const</span> user = <span style={{ color: colors.accent.purple }}>await</span> db.find(req.email);</div>
            <div>{'  '}<span style={{ color: colors.accent.purple }}>if</span> (!user) <span style={{ color: colors.accent.purple }}>throw new</span> NotFound();</div>
            <div style={{ color: colors.accent.red }}>{'  '}// Wrong: 24h instead of 15m</div>
            <div>{'  '}<span style={{ color: colors.accent.purple }}>return</span> jwt.sign(user, {'{'}</div>
            <div>{'    '}expiresIn: <span style={{ color: colors.accent.green }}>'24h'</span></div>
            <div>{'  '}{'}'});</div>
            <div style={{ color: colors.accent.red }}>{'  '}// auth_failed event never emitted</div>
            <div>{'}'}</div>
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
            {['Rate limiter missing (port unknown)', 'Wrong expiry (invariant invisible)', 'Event contract violated (edge unseen)'].map((t) => (
              <div key={t} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <div style={{ width: 7, height: 7, borderRadius: '50%', background: colors.accent.red, flexShrink: 0 }} />
                <Text fontSize="14px" color={colors.accent.red} margin="0">{t}</Text>
              </div>
            ))}
          </div>
        </div>

        {/* WITH */}
        <div style={{ flex: 1, background: colors.bg.card, borderRadius: 14, border: '1px solid rgba(255,255,255,0.06)', borderTop: `3px solid ${colors.accent.teal}`, padding: '14px 18px', display: 'flex', flexDirection: 'column', gap: 10, overflow: 'hidden' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <Text fontSize="18px" color={colors.accent.teal} margin="0" fontWeight="bold">With SpecForge</Text>
            <Text fontSize="14px" color={colors.text.muted} margin="0">2.4K tokens</Text>
          </div>
          <div style={{ flex: 1, background: '#0a0a12', borderRadius: 10, padding: '14px 16px', fontFamily: '"JetBrains Mono", monospace', fontSize: 15, lineHeight: 1.55, color: colors.text.secondary, overflow: 'hidden', whiteSpace: 'pre' }}>
            <div style={{ color: colors.text.muted }}>// Agent reads typed graph</div>
            <div>&nbsp;</div>
            <div><span style={{ color: colors.accent.purple }}>async</span> <span style={{ color: colors.accent.blue }}>authenticate</span>(req) {'{'}</div>
            <div style={{ color: colors.accent.teal }}>{'  '}// Rate limiter FIRST (port)</div>
            <div>{'  '}<span style={{ color: colors.accent.purple }}>const</span> ok = <span style={{ color: colors.accent.purple }}>await</span> this.rateLimiter.check(req.ip);</div>
            <div>{'  '}<span style={{ color: colors.accent.purple }}>if</span> (!ok) {'{'}</div>
            <div>{'    '}<span style={{ color: colors.accent.purple }}>await</span> this.emit(<span style={{ color: colors.accent.green }}>'auth_failed'</span>,</div>
            <div>{'      '}{'{'} reason: <span style={{ color: colors.accent.green }}>'rate_limit'</span> {'}'});</div>
            <div>{'    '}<span style={{ color: colors.accent.purple }}>throw new</span> TooManyRequests();</div>
            <div>{'  '}{'}'}</div>
            <div style={{ color: colors.accent.teal }}>{'  '}// 15m from invariant</div>
            <div>{'  '}<span style={{ color: colors.accent.purple }}>return</span> jwt.sign(user, {'{'}</div>
            <div>{'    '}expiresIn: <span style={{ color: colors.accent.green }}>'15m'</span> {'}'});</div>
            <div>{'}'}</div>
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
            {['Rate limiter first (port in graph)', 'Correct 15m (invariant enforced)', 'auth_failed emitted (event edge)'].map((t) => (
              <div key={t} style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                <div style={{ width: 7, height: 7, borderRadius: '50%', background: colors.accent.green, flexShrink: 0 }} />
                <Text fontSize="14px" color={colors.accent.green} margin="0">{t}</Text>
              </div>
            ))}
          </div>
        </div>
      </div>
    </FlexBox>
    <Notes>Target accuracy metric: ~30% first-attempt accuracy with prose context, targeting 70-85% with graph context. Based on research (Ambig-SWE, SWT-Bench) showing clarified requirements improve AI agent performance up to 74%. Each bullet maps directly to a graph concept: port, invariant, event edge.</Notes>
  </Slide>,
];
