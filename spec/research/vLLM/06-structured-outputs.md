# Chapter 06 — Structured outputs: forms the robot cannot fill in wrong

> **In one sentence:** you can hand vLLM a **mold** — a fill-in-the-blank form,
> a pattern, or even the full grammar of a language — and the robot will be
> **physically unable** to write an answer that does not fit the mold.

---

## The robot who sometimes goes rogue

Ada is brilliant, but she is improvising all the time. Ask her for a "yes or no"
and she might write a paragraph. Ask for a JSON file and she might forget a
bracket. Usually charming; occasionally a disaster — especially when **another
computer** has to read her answer.

vLLM has a superpower for this: while Ada writes **word by word**, vLLM stands
behind her like a teacher with a hand on the keyboard and quietly **blocks any
key that would break the form**. She is not corrected after the fact. The wrong
keys simply **do not press**.

> **Grown-up word:** this is **structured output** (or *guided/grammar-constrained
> decoding*). At each step, vLLM computes which tokens would be legal next and
> masks out all the others before picking. The default engine for this in vLLM
> is called **xgrammar**.

---

## The four molds, from simple to mighty

| Mold | What it forces | Kid version |
|------|----------------|-------------|
| **choice** | Answer must be one exact word from a list | "Pick door A, B, or C. The other doors are welded shut." |
| **regex** | Answer must match a letter pattern | "Your answer must look like a phone number, or you can't write it." |
| **json** | Answer must be a valid JSON object with the exact fields | "Fill in this form. Missing boxes? Impossible. Extra boxes? Impossible." |
| **grammar** | Answer must obey the grammar of a whole little language | "You may only write sentences of THIS language. Ever." |

The first three are for answers. The fourth one is a superpower, because a
"language" can be as small as *one file format*...

---

## ...which is exactly what SpecForge did with it

SpecForge has its own little language: **`.spec` files** — recipe files with
strict rules (`keyword name "title" { ... }`, and so on). The rules already
exist as a parser in this repository (`crates/kernel/tree-sitter/grammar.js`).

Idea: translate those rules **once** into a mold file (`specforge.gbnf`) and
hand it to vLLM. Now when a robot writes a `.spec` file, every single character
must fit the recipe-file rules. **The robot cannot produce a broken recipe.**
The expensive "oops, syntax error, try again" loop simply disappears.

```
Without mold:  robot writes → broken file → error → retry → maybe fixed (costs time + money)
With mold:     robot writes → file fits, guaranteed → check the meaning, done
```

The mold guarantees the **shape**. It cannot guarantee the robot said something
*smart* — checking meaning is SpecForge's own job (`specforge check`). Mold for
syntax, SpecForge for sense. Teamwork.

> **Check the evidence:** vLLM's structured-outputs feature and its four mold
> types are official documentation [E3]; the idea of shipping SpecForge's grammar
> as a mold is RES-31 Section 07 (the "flagship concept proof"), building on
> RES-29's grammar work.

---

> **Check yourself**
>
> 1. How does vLLM stop the robot from breaking the form?
> 2. Which mold is strongest: choice, regex, json, or grammar?
> 3. With a grammar mold, is the robot guaranteed to be *right*, or just *well-formed*?
>
> <details><summary>Answers</summary>
>
> 1. Before each word is chosen, illegal choices are masked out — wrong keys
>    cannot be pressed at all.
> 2. **grammar** — it can enforce an entire file format or language, which
>    includes what the simpler molds do.
> 3. Just well-formed. Syntax is guaranteed; meaning still needs checking
>    (that is `specforge check`'s job).
>
> </details>

---

**Previous:** [Chapter 05 — Prefix caching](05-prefix-caching.md) ·
**Next:** [Chapter 07 — The universal plug](07-universal-plug-and-embeddings.md)
