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
10.1109/ICASSP43922.2022.9746317
https://doi.org/10.1109/ICASSP48485.2024.10448020
http://arxiv.org/abs/2211.08553
arXiv.2306.09382  
https://dl.acm.org/doi/10.1109/TASLP.2023.3271145
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
