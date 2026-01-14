<div align="center">
  <h1>papersearch</h1>
  <h4>Map citation networks</h4>
  <figure>
    <img src="assets/image.png?raw=true" alt="Demo" />
  </figure>
</div>

This tool helps you explore connections between research papers.
From a set of seed papers, it pulls their references and citations from Semantic Scholar, and lets you explore the resulting graph locally.

This is quite useful for literature reviews, notably to find important papers in the field you might have missed.


## Installation

Requires Rust and Node.js.

```bash
npm --prefix webui install && npm --prefix webui run build
cargo install --path .
```

## Quick Start

To use this tool, you need to
- build a literature graph by querying semantic scholar
- visualize this graph in a local web ui.

### 1. Build a citation graph

```bash
papersearch lookup papers.txt -o graph.json
```

Input file: one paper identifier per line (DOI, arXiv ID, or URL).

<details>
<summary>Example input</summary>

```
https://doi.org/10.48550/arXiv.2201.05125
https://doi.org/10.48550/arXiv.2306.12700
https://doi.org/10.48550/arXiv.2307.04526
```
</details>


### 2. View the graph

```bash
papersearch view graph.json
```

Opens an interactive web UI where you can:
- Search and highlight papers
- Filter by citation counts
- Explore reference and citation relationships

Options:
```bash
papersearch view graph.json
```

