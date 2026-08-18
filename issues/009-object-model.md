---
id: 9
title: "Decide the object model"
type: grilling
status: open
assignee:
blocked-by: [7]
---

## Question

Graft's object kinds and encodings: what are the objects (blob/tree/change/…), what is the
content-address (hash choice, canonical encoding), how does stable change identity relate
to content hash, how are conflicts represented (first-class in the model à la jj, or
marker-files à la git), and — the governing constraint — which choices keep the door open
for a history-native substrate to grow underneath (ticket 2)? Also: where do
secrets-bearing objects hook in (interface only; detail in ticket 10)?
