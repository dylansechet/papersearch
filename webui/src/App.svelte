<script>
  import { onMount } from 'svelte';
  import ForceGraph from 'force-graph';
  import * as d3 from 'd3-force';
  import TomSelect from 'tom-select';
  import 'tom-select/dist/css/tom-select.css';

  let graphContainer;
  let graph;
  let fileInput;
  let searchInput;
  let tomSelect;
  let stats = $state(null);
  let rawData = $state(null);
  let minCitesSeeds = $state(1);
  let minCitedBySeeds = $state(1);
  let highlight = { node: null, neighbors: new Set() };
  let currentNodes = [];
  let currentEdges = [];

  function loadGraph() {
    fileInput.click();
  }

  function handleFileSelect(event) {
    const file = event.target.files?.[0];
    if (!file) return;

    const reader = new FileReader();
    reader.onload = (e) => {
      try {
        rawData = JSON.parse(e.target.result);
        const seedCount = rawData.seeds.length;
        const defaultMin = Math.max(1, Math.round(seedCount * 0.2));
        minCitesSeeds = defaultMin;
        minCitedBySeeds = defaultMin;
        visualizeGraph();
      } catch (error) {
        alert('Error parsing JSON: ' + error.message);
      }
    };
    reader.readAsText(file);
  }

  function visualizeGraph() {
    if (!rawData) return;
    const { papers, seeds, citations } = rawData;
    const seedSet = new Set(seeds);

    // Count how many times each paper cites seeds vs is cited by seeds
    const citesSeeds = new Map();
    const citedBySeeds = new Map();

    for (const c of citations) {
      if (seedSet.has(c.to) && !seedSet.has(c.from)) {
        citesSeeds.set(c.from, (citesSeeds.get(c.from) || 0) + 1);
      }
      if (seedSet.has(c.from) && !seedSet.has(c.to)) {
        citedBySeeds.set(c.to, (citedBySeeds.get(c.to) || 0) + 1);
      }
    }

    // Filter papers based on sliders
    const filteredPapers = papers.filter(paper => {
      if (seedSet.has(paper.id)) return true;  // Always keep seeds
      const cites = citesSeeds.get(paper.id) || 0;
      const cited = citedBySeeds.get(paper.id) || 0;
      if (cites > cited) return cites >= minCitesSeeds;  // Citing paper
      return cited >= minCitedBySeeds;  // Cited paper (includes ties)
    });

    const filteredIds = new Set(filteredPapers.map(p => p.id));
    const paperMap = new Map(filteredPapers.map(p => [p.id, p]));

    const nodes = filteredPapers.map(paper => {
      const id = paper.id;
      let color, type;

      if (seedSet.has(id)) {
        color = '#ff6b6b';
        type = 'square';
      } else {
        const cites = citesSeeds.get(id) || 0;
        const cited = citedBySeeds.get(id) || 0;
        if (cites > cited) {
          color = '#9b59b6';
          type = 'triangle';
        } else {
          color = '#3498db';
          type = 'circle';
        }
      }

      return { id, color, type, paper };
    });

    const edges = citations
      .filter(c => filteredIds.has(c.from) && filteredIds.has(c.to))
      .map(c => ({ source: c.from, target: c.to }));

    // Count degree (edges) per node for sizing
    const degree = new Map();
    edges.forEach(e => {
      degree.set(e.source, (degree.get(e.source) || 0) + 1);
      degree.set(e.target, (degree.get(e.target) || 0) + 1);
    });

    // Find max degree per category
    let maxDegreeSeed = 1, maxDegreeCites = 1, maxDegreeCited = 1;
    nodes.forEach(n => {
      const deg = degree.get(n.id) || 0;
      if (seedSet.has(n.id)) {
        maxDegreeSeed = Math.max(maxDegreeSeed, deg);
      } else if (n.type === 'triangle') {
        maxDegreeCites = Math.max(maxDegreeCites, deg);
      } else {
        maxDegreeCited = Math.max(maxDegreeCited, deg);
      }
    });

    // Add size based on degree relative to category max
    nodes.forEach(n => {
      const baseSize = 5;
      const deg = degree.get(n.id) || 0;
      let maxDeg;
      if (seedSet.has(n.id)) maxDeg = maxDegreeSeed;
      else if (n.type === 'triangle') maxDeg = maxDegreeCites;
      else maxDeg = maxDegreeCited;
      n.size = baseSize + (deg / maxDeg) * 3;
    });

    stats = { papers: filteredPapers.length, seeds: seeds.length, citations: edges.length };
    currentNodes = nodes;
    currentEdges = edges;
    updateSearchOptions();

    if (graph) graph._destructor();

    graph = ForceGraph()(graphContainer)
      .graphData({ nodes, links: edges })
      .linkColor(link => {
        if (!highlight.node) return '#ddd';
        const src = typeof link.source === 'object' ? link.source.id : link.source;
        const tgt = typeof link.target === 'object' ? link.target.id : link.target;
        return (src === highlight.node || tgt === highlight.node) ? '#999' : 'rgba(200,200,200,0.1)';
      })
      .linkDirectionalArrowLength(2)
      .onNodeClick((node) => {
        if (highlight.node === node.id) {
          highlight.node = null;
          highlight.neighbors = new Set();
        } else {
          highlight.node = node.id;
          highlight.neighbors = new Set();
          edges.forEach(e => {
            const src = typeof e.source === 'object' ? e.source.id : e.source;
            const tgt = typeof e.target === 'object' ? e.target.id : e.target;
            if (src === node.id) highlight.neighbors.add(tgt);
            if (tgt === node.id) highlight.neighbors.add(src);
          });
        }
        // Trigger re-render by re-setting accessor
        graph.nodeCanvasObject(graph.nodeCanvasObject()).linkColor(graph.linkColor());
      })
      .onBackgroundClick(() => {
        highlight.node = null;
        highlight.neighbors = new Set();
        // Trigger re-render by re-setting accessor
        graph.nodeCanvasObject(graph.nodeCanvasObject()).linkColor(graph.linkColor());
      })
      .d3Force('charge', d3.forceManyBody().strength(-50))
      .d3Force('link', d3.forceLink().distance(60))
      .nodeCanvasObject((node, ctx, globalScale) => {
        const size = node.size;
        const isHighlighted = !highlight.node || highlight.node === node.id || highlight.neighbors.has(node.id);
        ctx.globalAlpha = isHighlighted ? 1 : 0.15;
        ctx.fillStyle = node.color;
        ctx.strokeStyle = '#fff';
        ctx.lineWidth = 1 / globalScale;

        if (node.type === 'square') {
          ctx.fillRect(node.x - size, node.y - size, size * 2, size * 2);
          ctx.strokeRect(node.x - size, node.y - size, size * 2, size * 2);
        } else if (node.type === 'triangle') {
          ctx.beginPath();
          ctx.moveTo(node.x, node.y - size);
          ctx.lineTo(node.x + size, node.y + size);
          ctx.lineTo(node.x - size, node.y + size);
          ctx.closePath();
          ctx.fill();
          ctx.stroke();
        } else {
          ctx.beginPath();
          ctx.arc(node.x, node.y, size, 0, 2 * Math.PI);
          ctx.fill();
          ctx.stroke();
        }
        ctx.globalAlpha = 1;
      })
      .nodePointerAreaPaint((node, color, ctx) => {
        ctx.fillStyle = color;
        ctx.beginPath();
        ctx.arc(node.x, node.y, node.size, 0, 2 * Math.PI);
        ctx.fill();
      })
      .nodeLabel(n => {
        const p = n.paper;
        const year = p.year ? ` (${p.year})` : '';
        const authors = p.authors?.join(', ') || 'Unknown';
        const venue = p.venue ? `<br><i>${p.venue}</i>` : '';
        return `<b>${p.title || n.id}${year}</b><br>${authors}${venue}`;
      })
      .cooldownTicks(200);
  }

  function updateSearchOptions() {
    if (!tomSelect) return;
    tomSelect.clear();
    tomSelect.clearOptions();
    currentNodes.forEach(n => {
      const p = n.paper;
      const year = p.year ? ` (${p.year})` : '';
      tomSelect.addOption({ value: n.id, text: `${p.title || n.id}${year}` });
    });
  }

  function highlightNode(nodeId) {
    if (!graph || !nodeId) return;
    highlight.node = nodeId;
    highlight.neighbors = new Set();
    currentEdges.forEach(e => {
      const src = typeof e.source === 'object' ? e.source.id : e.source;
      const tgt = typeof e.target === 'object' ? e.target.id : e.target;
      if (src === nodeId) highlight.neighbors.add(tgt);
      if (tgt === nodeId) highlight.neighbors.add(src);
    });
    graph.nodeCanvasObject(graph.nodeCanvasObject()).linkColor(graph.linkColor());
  }

  onMount(() => {
    tomSelect = new TomSelect(searchInput, {
      placeholder: 'Search papers...',
      onChange: (value) => {
        if (value) highlightNode(value);
      }
    });
    return () => {
      graph?._destructor();
      tomSelect?.destroy();
    };
  });
</script>

<header>
  <button onclick={loadGraph}>Load Graph</button>
  <input type="file" accept=".json" bind:this={fileInput} onchange={handleFileSelect} hidden />
  <select bind:this={searchInput}></select>
  {#if rawData}
    <label class="slider-group slider-cites">
      Cites ≥ {minCitesSeeds}
      <input type="range" min="1" max="10" bind:value={minCitesSeeds} onchange={visualizeGraph} />
    </label>
    <label class="slider-group slider-cited">
      Cited ≥ {minCitedBySeeds}
      <input type="range" min="1" max="10" bind:value={minCitedBySeeds} onchange={visualizeGraph} />
    </label>
  {/if}
  {#if stats}
    <span class="stats">{stats.papers} papers · {stats.seeds} seeds · {stats.citations} citations</span>
  {/if}
</header>
<div id="graph" bind:this={graphContainer}></div>

<div class="legend">
  <div class="legend-item"><span class="legend-square" style="background:#ff6b6b;"></span> Seed</div>
  <div class="legend-item"><span class="legend-triangle"></span> Cites seeds</div>
  <div class="legend-item"><span class="legend-circle" style="background:#3498db;"></span> Cited by seeds</div>
</div>

<style>
  header {
    height: 50px;
    padding: 0 1rem;
    display: flex;
    align-items: center;
    gap: 1rem;
    background: #f5f5f5;
    border-bottom: 1px solid #ddd;
  }

  header :global(.ts-wrapper) {
    min-width: 250px;
  }

  header :global(.ts-control) {
    padding: 0.3rem 0.5rem;
    font-size: 0.85rem;
  }

  button {
    padding: 0.4rem 0.8rem;
    font-weight: normal;
  }

  .slider-group {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.85rem;
  }

  .slider-cites {
    color: #9b59b6;
  }

  .slider-cites input {
    accent-color: #9b59b6;
  }

  .slider-cited {
    color: #3498db;
  }

  .slider-cited input {
    accent-color: #3498db;
  }

  .slider-group input {
    width: 80px;
  }

  .stats {
    color: #666;
    font-size: 0.9rem;
    margin-left: auto;
  }

  #graph {
    height: calc(100vh - 50px);
  }

  .legend {
    position: absolute;
    bottom: 1rem;
    left: 1rem;
    background: rgba(255,255,255,0.9);
    padding: 0.75rem;
    border-radius: 4px;
    border: 1px solid #ddd;
    font-size: 0.8rem;
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.25rem;
  }

  .legend-item:last-child {
    margin-bottom: 0;
  }

  .legend-square {
    width: 12px;
    height: 12px;
  }

  .legend-circle {
    width: 12px;
    height: 12px;
    border-radius: 50%;
  }

  .legend-triangle {
    width: 0;
    height: 0;
    border-left: 6px solid transparent;
    border-right: 6px solid transparent;
    border-bottom: 12px solid #9b59b6;
  }
</style>
