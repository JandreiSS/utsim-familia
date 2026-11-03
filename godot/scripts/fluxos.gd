## The token flows of the model in 2D (phase G3): each flow (`Model.flows`) as a graph of
## its activities (layered by the bridge along `next`), the tokens in each activity at the
## playback time written on the box, and below the counts of every activity over the whole
## run, with a line at the current time. Data from `UtsimPlayback.flows()`,
## `flow_counts(k, t)` and `flow_series(k, n)`.
class_name PainelFluxos
extends PanelContainer

const SAMPLES := 300
const PALETTE := [Color(0.35, 0.59, 0.94), Color(0.98, 0.80, 0.35), Color(0.45, 0.85, 0.50),
	Color(1.0, 0.55, 0.45), Color(0.75, 0.55, 0.95), Color(0.40, 0.85, 0.90), Color(0.90, 0.70, 0.55)]

var main: Node
var playback: Object
var flows: Array = []
var flow := 0
var counts := PackedInt32Array()
var _pick: OptionButton
var _graph: Grafo
var _chart: Grafico
var _series: Dictionary = {}
var _note: Label


func build(owner_main: Node) -> void:
	main = owner_main
	name = "PainelFluxos"
	var root := VBoxContainer.new()
	add_child(root)
	var head := HBoxContainer.new()
	root.add_child(head)
	var title := Label.new()
	title.text = "Fluxos de tokens"
	title.add_theme_font_size_override("font_size", 18)
	head.add_child(title)
	_pick = OptionButton.new()
	_pick.name = "Fluxos_Escolha"
	_pick.focus_mode = Control.FOCUS_NONE
	_pick.item_selected.connect(func(k): _choose(k))
	head.add_child(_pick)
	var sp := Control.new()
	sp.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(sp)
	var close := Button.new()
	close.name = "Fluxos_Fechar"
	close.text = "Fechar"
	close.focus_mode = Control.FOCUS_NONE
	close.pressed.connect(func(): visible = false)
	head.add_child(close)
	_note = Label.new()
	_note.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	_note.add_theme_color_override("font_color", Color(0.70, 0.74, 0.80))
	root.add_child(_note)
	_graph = Grafo.new()
	_graph.name = "Fluxos_Grafo"
	_graph.custom_minimum_size = Vector2(0, 220)
	_graph.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root.add_child(_graph)
	_chart = Grafico.new()
	_chart.name = "Fluxos_Grafico"
	_chart.custom_minimum_size = Vector2(0, 180)
	_chart.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root.add_child(_chart)


## A new playback: its flows (empty: the panel says so).
func set_playback(pb: Object) -> void:
	playback = pb
	flows = pb.flows() if pb != null else []
	_pick.clear()
	for k in flows.size():
		_pick.add_item(flows[k]["name"], k)
	_choose(0)


func _choose(k: int) -> void:
	flow = k
	_series = {}
	if flows.is_empty():
		_note.text = "O modelo não tem fluxos de tokens."
		_graph.set_graph({}, PackedInt32Array())
		_chart.clear()
		return
	_note.text = "Tokens em cada atividade no instante da reprodução; embaixo, as contagens ao longo da rodada (a linha laranja é o instante atual)."
	_series = playback.flow_series(k, SAMPLES)
	show_time(main.t if main != null else 0.0)


## Updates the counts at playback time `t`.
func show_time(t: float) -> void:
	if playback == null or flows.is_empty() or not visible:
		return
	counts = playback.flow_counts(flow, t)
	_graph.set_graph(flows[flow], counts)
	var series := []
	var nodes: Array = flows[flow]["nodes"]
	var times: PackedFloat64Array = _series.get("times", PackedFloat64Array())
	var all: Array = _series.get("counts", [])
	for a in mini(nodes.size(), all.size()):
		var row: PackedInt32Array = all[a]
		var pts := []
		for i in mini(times.size(), row.size()):
			pts.append([times[i], float(row[i])])
		series.append({"points": pts, "color": PALETTE[a % PALETTE.size()], "width": 1.5})
	_chart.lines(series, {"markers": [{"x": t, "label": "t"}], "x_label": "tempo", "y_label": "tokens"})


## Total tokens at the last `show_time` (scripted checks).
func total_tokens() -> int:
	var n := 0
	for c in counts:
		n += c
	return n


## Most tokens any activity held over the run (scripted checks).
func peak_tokens() -> int:
	var n := 0
	for row in _series.get("counts", []):
		for c in row:
			n = maxi(n, c)
	return n


## The flow graph: boxes for the activities, arrows along `next`, the tokens on each box.
class Grafo extends Control:
	var g: Dictionary = {}
	var c := PackedInt32Array()

	func set_graph(graph: Dictionary, counts: PackedInt32Array) -> void:
		g = graph
		c = counts
		queue_redraw()

	func _draw() -> void:
		draw_rect(Rect2(Vector2.ZERO, size), Color(0.09, 0.10, 0.12))
		var nodes: Array = g.get("nodes", [])
		if nodes.is_empty():
			return
		var lo := Vector2(INF, INF)
		var hi := Vector2(-INF, -INF)
		for n in nodes:
			var p := Vector2(float(n["x"]), float(n["y"]))
			lo = lo.min(p)
			hi = hi.max(p)
		var span := (hi - lo).max(Vector2(1, 1))
		var box := Vector2(130, 40)
		var area := size - box - Vector2(20, 20)
		var at := func(i: int) -> Vector2:
			var n: Dictionary = nodes[i]
			var f := (Vector2(float(n["x"]), float(n["y"])) - lo) / span
			return Vector2(10, 10) + f * area.max(Vector2.ZERO) + box * 0.5
		var font := ThemeDB.fallback_font
		var edges: PackedInt32Array = g.get("edges", PackedInt32Array())
		for k in range(0, edges.size() - 1, 2):
			var a: Vector2 = at.call(edges[k])
			var b: Vector2 = at.call(edges[k + 1])
			var d := (b - a).normalized()
			a += d * box.x * 0.5
			b -= d * box.x * 0.5
			draw_line(a, b, Color(0.55, 0.60, 0.68), 1.5, true)
			var s := Vector2(-d.y, d.x) * 4.0
			draw_colored_polygon(PackedVector2Array([b, b - d * 9 + s, b - d * 9 - s]), Color(0.55, 0.60, 0.68))
		for i in nodes.size():
			var p: Vector2 = at.call(i)
			var r := Rect2(p - box * 0.5, box)
			var tokens := c[i] if i < c.size() else 0
			draw_rect(r, Color(0.16, 0.18, 0.22))
			draw_rect(r, Color(0.98, 0.80, 0.35) if tokens > 0 else Color(0.40, 0.45, 0.52), false, 2.0 if tokens > 0 else 1.0)
			draw_string(font, r.position + Vector2(6, 15), String(nodes[i]["name"]), HORIZONTAL_ALIGNMENT_LEFT, box.x - 12, 12, Color(0.88, 0.90, 0.95))
			draw_string(font, r.position + Vector2(6, 32), "%s: %d" % [nodes[i]["label"], tokens], HORIZONTAL_ALIGNMENT_LEFT, box.x - 12, 11, Color(0.98, 0.80, 0.35) if tokens > 0 else Color(0.62, 0.66, 0.72))
