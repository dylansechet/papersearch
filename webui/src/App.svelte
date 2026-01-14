<script>
  import { onMount } from 'svelte';
  import ForceGraph from 'force-graph';
  import { forceManyBody, forceLink } from 'd3-force';
  import TomSelect from 'tom-select';
  import 'tom-select/dist/css/tom-select.css';

  // Constants
  const COLORS = {
    seed: '#ff6b6b',
    citesSeed: '#9b59b6',
    citedBySeed: '#3498db',
    linkDefault: '#ddd',
    linkHighlight: '#999',
    linkFaded: 'rgba(200,200,200,0.1)',
  };

  const NODE_TYPES = {
    seed: 'square',
    citesSeed: 'triangle',
    citedBySeed: 'circle',
  };

  const GRAPH_CONFIG = {
    baseNodeSize: 5,
    maxNodeSizeBonus: 3,
    chargeStrength: -50,
    linkDistance: 60,
    cooldownTicks: 200,
    arrowLength: 3,
  };

  // DOM refs
  let graphContainer;
  let fileInput;
  let searchInput;

  // Instance refs
  let graph;
  let tomSelect;

  // Reactive state
  let stats = $state(null);
  let rawData = $state(null);
  let minCitesSeeds = $state(1);
  let minCitedBySeeds = $state(1);

  // Internal state
  let highlight = { node: null, neighbors: new Set() };
  let currentNodes = [];
  let currentEdges = [];

  // --- Utility Functions ---

  /** Extract node ID from edge endpoint (handles both string IDs and node objects) */
  function getEdgeNodeId(endpoint) {
    return typeof endpoint === 'object' ? endpoint.id : endpoint;
  }

  /** Get neighbors of a node from edge list */
  function findNeighbors(nodeId, edges) {
    const neighbors = new Set();
    for (const edge of edges) {
      const src = getEdgeNodeId(edge.source);
      const tgt = getEdgeNodeId(edge.target);
      if (src === nodeId) neighbors.add(tgt);
      if (tgt === nodeId) neighbors.add(src);
    }
    return neighbors;
  }

  /** Format paper info for display */
  function formatPaperLabel(paper, nodeId) {
    const year = paper.year ? ` (${paper.year})` : '';
    const authors = paper.authors?.join(', ') || 'Unknown';
    const venue = paper.venue ? `<br><i>${paper.venue}</i>` : '';
    return `<b>${paper.title || nodeId}${year}</b><br>${authors}${venue}`;
  }

  /** Format paper for search dropdown */
  function formatPaperOption(paper, nodeId) {
    const year = paper.year ? ` (${paper.year})` : '';
    return `${paper.title || nodeId}${year}`;
  }

  // --- Data Processing ---

  /** Count citation relationships between seeds and other papers */
  function computeCitationCounts(citations, seedSet) {
    const citesSeeds = new Map();    // Papers that cite seeds
    const citedBySeeds = new Map();  // Papers cited by seeds

    for (const { from, to } of citations) {
      // Non-seed paper cites a seed
      if (seedSet.has(to) && !seedSet.has(from)) {
        citesSeeds.set(from, (citesSeeds.get(from) || 0) + 1);
      }
      // Seed cites a non-seed paper
      if (seedSet.has(from) && !seedSet.has(to)) {
        citedBySeeds.set(to, (citedBySeeds.get(to) || 0) + 1);
      }
    }

    return { citesSeeds, citedBySeeds };
  }

  /** Filter papers based on citation thresholds */
  function filterPapers(papers, seedSet, citesSeeds, citedBySeeds) {
    return papers.filter(paper => {
      if (seedSet.has(paper.id)) return true;

      const cites = citesSeeds.get(paper.id) || 0;
      const cited = citedBySeeds.get(paper.id) || 0;

      // Classify by dominant relationship
      return cites > cited
        ? cites >= minCitesSeeds
        : cited >= minCitedBySeeds;
    });
  }

  /** Determine node category based on citation relationships */
  function getNodeCategory(paperId, seedSet, citesSeeds, citedBySeeds) {
    if (seedSet.has(paperId)) return 'seed';

    const cites = citesSeeds.get(paperId) || 0;
    const cited = citedBySeeds.get(paperId) || 0;

    return cites > cited ? 'citesSeed' : 'citedBySeed';
  }

  /** Build graph nodes from papers */
  function buildNodes(papers, seedSet, citesSeeds, citedBySeeds) {
    return papers.map(paper => {
      const category = getNodeCategory(paper.id, seedSet, citesSeeds, citedBySeeds);
      return {
        id: paper.id,
        color: COLORS[category],
        type: NODE_TYPES[category],
        category,
        paper,
      };
    });
  }

  /** Build edges from citations, filtered to included papers */
  function buildEdges(citations, includedIds) {
    return citations
      .filter(c => includedIds.has(c.from) && includedIds.has(c.to))
      .map(c => ({ source: c.from, target: c.to }));
  }

  /** Compute node degrees from edges */
  function computeDegrees(edges) {
    const degree = new Map();
    for (const { source, target } of edges) {
      degree.set(source, (degree.get(source) || 0) + 1);
      degree.set(target, (degree.get(target) || 0) + 1);
    }
    return degree;
  }

  /** Calculate node sizes based on relative degree within category */
  function assignNodeSizes(nodes, degree) {
    // Find max degree per category
    const maxDegreeByCategory = { seed: 1, citesSeed: 1, citedBySeed: 1 };

    for (const node of nodes) {
      const deg = degree.get(node.id) || 0;
      maxDegreeByCategory[node.category] = Math.max(maxDegreeByCategory[node.category], deg);
    }

    // Assign sizes
    for (const node of nodes) {
      const deg = degree.get(node.id) || 0;
      const maxDeg = maxDegreeByCategory[node.category];
      node.size = GRAPH_CONFIG.baseNodeSize + (deg / maxDeg) * GRAPH_CONFIG.maxNodeSizeBonus;
    }
  }

  // --- Highlighting ---

  function clearHighlight() {
    highlight.node = null;
    highlight.neighbors = new Set();
  }

  function setHighlight(nodeId, edges) {
    highlight.node = nodeId;
    highlight.neighbors = findNeighbors(nodeId, edges);
  }

  function refreshGraphHighlight() {
    if (!graph) return;
    graph.nodeCanvasObject(graph.nodeCanvasObject()).linkColor(graph.linkColor());
  }

  function highlightNode(nodeId) {
    if (!graph || !nodeId) return;
    setHighlight(nodeId, currentEdges);
    refreshGraphHighlight();
  }

  // --- Graph Rendering ---

  function getLinkColor(link) {
    if (!highlight.node) return COLORS.linkDefault;

    const src = getEdgeNodeId(link.source);
    const tgt = getEdgeNodeId(link.target);
    const isConnected = src === highlight.node || tgt === highlight.node;

    return isConnected ? COLORS.linkHighlight : COLORS.linkFaded;
  }

  function drawNode(node, ctx, globalScale) {
    const { x, y, size, color, type } = node;
    const isHighlighted = !highlight.node || highlight.node === node.id || highlight.neighbors.has(node.id);

    ctx.globalAlpha = isHighlighted ? 1 : 0.15;
    ctx.fillStyle = color;
    ctx.strokeStyle = '#fff';
    ctx.lineWidth = 1 / globalScale;

    switch (type) {
      case 'square':
        ctx.fillRect(x - size, y - size, size * 2, size * 2);
        ctx.strokeRect(x - size, y - size, size * 2, size * 2);
        break;
      case 'triangle':
        ctx.beginPath();
        ctx.moveTo(x, y - size);
        ctx.lineTo(x + size, y + size);
        ctx.lineTo(x - size, y + size);
        ctx.closePath();
        ctx.fill();
        ctx.stroke();
        break;
      default: // circle
        ctx.beginPath();
        ctx.arc(x, y, size, 0, 2 * Math.PI);
        ctx.fill();
        ctx.stroke();
    }

    ctx.globalAlpha = 1;
  }

  function drawNodeHitArea(node, color, ctx) {
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.arc(node.x, node.y, node.size, 0, 2 * Math.PI);
    ctx.fill();
  }

  function createGraph(nodes, edges) {
    if (graph) graph._destructor();

    graph = ForceGraph()(graphContainer)
      .graphData({ nodes, links: edges })
      .linkColor(getLinkColor)
      .linkDirectionalArrowLength(GRAPH_CONFIG.arrowLength)
      .d3Force('charge', forceManyBody().strength(GRAPH_CONFIG.chargeStrength))
      .d3Force('link', forceLink().distance(GRAPH_CONFIG.linkDistance))
      .nodeCanvasObject(drawNode)
      .nodePointerAreaPaint(drawNodeHitArea)
      .nodeLabel(n => formatPaperLabel(n.paper, n.id))
      .cooldownTicks(GRAPH_CONFIG.cooldownTicks)
      .onNodeClick(node => {
        if (highlight.node === node.id) {
          clearHighlight();
        } else {
          setHighlight(node.id, edges);
        }
        refreshGraphHighlight();
      })
      .onBackgroundClick(() => {
        clearHighlight();
        refreshGraphHighlight();
      });
  }

  // --- Search ---

  function updateSearchOptions() {
    if (!tomSelect) return;

    tomSelect.clear();
    tomSelect.clearOptions();

    for (const node of currentNodes) {
      tomSelect.addOption({
        value: node.id,
        text: formatPaperOption(node.paper, node.id),
      });
    }
  }

  // --- Main Functions ---

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

        // Set default filter thresholds based on seed count
        const defaultMin = Math.max(1, Math.round(rawData.seeds.length * 0.2));
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

    // Process data
    const { citesSeeds, citedBySeeds } = computeCitationCounts(citations, seedSet);
    const filteredPapers = filterPapers(papers, seedSet, citesSeeds, citedBySeeds);
    const filteredIds = new Set(filteredPapers.map(p => p.id));

    // Build graph data
    const nodes = buildNodes(filteredPapers, seedSet, citesSeeds, citedBySeeds);
    const edges = buildEdges(citations, filteredIds);

    // Size nodes by degree
    const degree = computeDegrees(edges);
    assignNodeSizes(nodes, degree);

    // Update state
    currentNodes = nodes;
    currentEdges = edges;
    stats = { papers: filteredPapers.length, seeds: seeds.length, citations: edges.length };
    clearHighlight();

    // Render
    updateSearchOptions();
    createGraph(nodes, edges);
  }

  function handleKeydown(event) {
    if (event.key === 'Escape') {
      clearHighlight();
      refreshGraphHighlight();
      tomSelect?.clear();
    }
  }

  onMount(() => {
    tomSelect = new TomSelect(searchInput, {
      placeholder: 'Search papers...',
      onChange: value => value && highlightNode(value),
    });

    window.addEventListener('keydown', handleKeydown);

    return () => {
      window.removeEventListener('keydown', handleKeydown);
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
