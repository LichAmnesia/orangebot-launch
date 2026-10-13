# evals

`cleanup.jsonl` — fixed cases for the text-polish step (dictate / translate / ask). Each row:
`id`, `mode` (dictate|translate|ask), optional `target`, `input` (an ASR transcript), and checks on the model output:
`must` (all substrings present), `must_any` (at least one present, skipped when empty), `must_not` (none present).
Matching is case-insensitive. Run with `ob eval cleanup --llm-url … --llm-model …`.
Cases target the spec's G2/G3 gates: self-corrections, fillers, numbers/money/names kept exact, no answering, no translating in dictate mode.
