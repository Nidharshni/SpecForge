# Chapter 10 — vLLM and SpecForge: the two doors and one gift

> **In one sentence:** SpecForge is a **recipe-checker**, not a kitchen — so
> vLLM can only enter through **two side doors** (both outside the checker), and
> the best move is a **gift**: publishing SpecForge's grammar as a mold so any
> vLLM kitchen writes perfect recipes automatically.

---

## First, what SpecForge is (thirty seconds)

SpecForge turns messy human intentions into **`.spec` recipe files** — a strict,
checkable format — and then hands AI agents a clean, validated **map of the
project** so they stop guessing. The checker itself (the compiler) is
deliberately simple and boring: parse, check, export. **No AI inside.** That is
not laziness; it is Rule 7 of the house ("make it an extension, never a
built-in") and Rule 8 ("a new user must get value in 60 seconds, with zero
setup").

A GPU kitchen is the *opposite* of zero-setup. So vLLM can never live inside
SpecForge's toolbox. But two side doors are wide open:

---

## Door 1 — the mapmaker (embeddings)

SpecForge wants agents to search recipes **by meaning** ("what talks about
payments?"). Chapter 07 taught you the trick: turn recipes into points on a
meaning-map. SpecForge defines a **port** — a wall socket called
`EmbeddingProvider` — that says "plug ANY mapmaker here." The socket is already
in the blueprints; nobody has plugged anything in yet.

vLLM is one possible mapmaker to plug in (it speaks the universal plug format,
Chapter 07). So are Ollama, LM Studio, or a paid API. **The socket does not
care.** That is the point of a socket.

> Reality check from Chapter 08: mapping the whole recipe book costs ~1 cent via
> an API. Door 1 exists for teams with **secret recipes** who need the mapmaking
> to happen **inside their own building** — not for speed, not for savings.

## Door 2 — the big batch job (infer)

`specforge infer` is a big job: an AI agent walks through a codebase, file by
file — thousands of little work orders — writing recipes for each. The *agent's*
brain has to run *somewhere*:

- **at a burger stand** (managed API) — easy, per-burger fees, your code travels;
- **in your own vLLM kitchen** — fixed cost, and with ~3,100–5,000 work orders a
  month it beats the big stands on price too (Chapter 08), plus zero secrets
  leave the house.

The batch shape is *perfect* for vLLM's tricks: thousands of similar orders
(never-idle chef, Chapter 04), a shared house-rules card (~20% of every order —
prefix caching, Chapter 05), and exactly the crowd where Ollama wobbles
(Chapter 09).

---

## The gift: `specforge.gbnf`

And now the best idea in the whole evaluation (RES-31 calls it the flagship).

SpecForge already owns the rules of recipe files — its parser
(`crates/kernel/tree-sitter/grammar.js`) is the rulebook. Chapter 06 taught
you that vLLM can enforce **any rulebook** while the robot writes. So:

1. Translate the rulebook **once** into a mold file: `specforge.gbnf`.
2. Ship the mold with the recipe extension.
3. Any agent using any vLLM (or SGLang) kitchen attaches the mold — and from
   then on, **every recipe the robot writes fits the file format perfectly.**
   Guaranteed. Every time.

```
Before:  robot writes recipe → syntax oops → error → retry → ... (time + money)
After:   robot writes recipe → fits, always → specforge check → done
```

Why this is the best move and not the doors: it costs **zero GPUs**, it helps
**every agent in the world**, not just teams with big machines, and it turns
SpecForge from "the checker that catches mistakes" into "the mold that prevents
them." It even follows the house rules — the mold is an *artifact* (a file the
extension ships), not engine code inside the compiler.

> **Grown-up note:** the mold guarantees **shape**, never **sense**.
> `specforge check` remains the judge of meaning (references, orphans, cycles).
> And there is one honest caveat: mold-writing can slightly change *which* words
> the robot picks inside the legal ones, so RES-31 gates this on a measured
> trial (KPI STR-1: 500 generations, 100% must parse clean) before calling it
> "recommended" instead of "experimental".

---

## The verdict, in kid words

1. **Keep the checker boring.** No kitchen inside the toolbox. (House Rule 7.)
2. **Door 1 (mapmaker socket):** build it as a standard plug; teams with secret
   recipes can plug vLLM into it.
3. **Door 2 (big batch):** write a recipe — er, a *guide* — for teams running
   their own vLLM kitchen; let them self-qualify with the Chapter 08 numbers.
4. **Ship the gift first.** The mold (`specforge.gbnf`) is cheap, vendor-neutral,
   and helps everyone.

That is exactly what the grown-up dossier RES-31 recommends — now you know why.

---

> **Check yourself**
>
> 1. Why can vLLM never live *inside* SpecForge's compiler?
> 2. What are the two doors, and who is each one for?
> 3. What does the mold guarantee, and what does it NOT guarantee?
>
> <details><summary>Answers</summary>
>
> 1. House rules: everything is an extension (never a built-in) and the product
>    must give value in 60 seconds with zero setup — a GPU kitchen breaks both.
> 2. Door 1 = meaning-search mapmaking behind the `EmbeddingProvider` socket,
>    for teams with private recipes. Door 2 = self-hosted kitchens for big
>    `specforge infer` batch jobs — privacy and, at scale, price.
> 3. Shape: the file will always parse. NOT sense: it can still say something
>    wrong, which `specforge check` must catch.
>
> </details>

---

**Previous:** [Chapter 09 — The great kitchen race](09-the-great-kitchen-race.md) ·
**Back to:** [README](README.md) ·
**Grown-up dossier:** [RES-31 — vLLM Inference Evaluation](../RES-31-vllm-inference-evaluation.html)
