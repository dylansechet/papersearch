<div align="center">
  <h1>papersearch</h1>
  <figure>
    <img src="assets/image.png?raw=true" alt="Demo" />
  </figure>
</div>
<br />

This tool helps you explore connections between research papers.
From a set of seed papers, it pulls their references and citations from Semantic Scholar, and lets you explore the resulting graph locally.

This is quite useful for literature reviews, notably to find important papers in the field you might have missed.

## Installation

Download the latest release [here](https://github.com/dylansechet/papersearch/releases/latest).

## Quickstart

Generate the reference graph:

```bash
papersearch lookup papers.txt -o graph.json
```


<details>
<summary>Example input</summary>

```
https://doi.org/10.48550/arXiv.2201.05125
https://doi.org/10.48550/arXiv.2306.12700
https://doi.org/10.48550/arXiv.2307.04526
```
</details>

View it in the web ui:

```bash
papersearch view graph.json
```

## Building from source

Requires rust and node.js.

```bash
npm --prefix webui install && npm --prefix webui run build
cargo install --path .
```
