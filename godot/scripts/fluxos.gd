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
var _scroll: ScrollContainer
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
	var zoom_row := HBoxContainer.new()
	root.add_child(zoom_row)
	var hint := Label.new()
	hint.text = "Zoom (Ctrl + roda no grafo):"
	hint.add_theme_color_override("font_color", Color(0.70, 0.74, 0.80))
	zoom_row.add_child(hint)
	for z in [["Fluxos_ZoomMenos", "−", 1.0 / 1.25], ["Fluxos_ZoomMais", "+", 1.25], ["Fluxos_Ajustar", "Ajustar", 0.0]]:
		var b := Button.new()
		b.name = z[0]
		b.text = z[1]
		b.focus_mode = Control.FOCUS_NONE
		var factor: float = z[2]
		b.pressed.connect(func(): _graph.fit() if factor == 0.0 else _graph.set_zoom(_graph.zoom * factor))
		zoom_row.add_child(b)
	_scroll = ScrollContainer.new()
	_scroll.name = "Fluxos_Rolagem"
	_scroll.custom_minimum_size = Vector2(0, 220)
	_scroll.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	root.add_child(_scroll)
	_graph = Grafo.new()
	_graph.name = "Fluxos_Grafo"
	_graph.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_graph.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_scroll.add_child(_graph)
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
## The bridge gives each activity its column (`layer`, the longest path from the sources)
## and its place in the column (`lane`); the boxes are as wide as the longest text, so
## nothing is cut, and the graph keeps that size: it scrolls in its `ScrollContainer`
## and zooms (the buttons, Ctrl + wheel). A new graph starts fitted to the width, but
## never below `MIN_FIT` (the text stays legible; the rest scrolls).
class Grafo extends Control:
	const NAME_SIZE := 13
	const LABEL_SIZE := 12
	const PAD := 8.0
	const GAP := Vector2(46, 18)
	const MIN_FIT := 0.75
	var g: Dictionary = {}
	var c := PackedInt32Array()
	var zoom := 1.0
	## Box of each activity, in graph coordinates (before the zoom).
	var boxes: Array[Rect2] = []
	var content := Vector2.ZERO
	var _shown := ""

	func set_graph(graph: Dictionary, counts: PackedInt32Array) -> void:
		var fresh: bool = String(graph.get("name", "")) != _shown or graph.get("nodes", []).size() != boxes.size()
		g = graph
		c = counts
		if fresh:
			_shown = String(graph.get("name", ""))
			_layout()
			fit()
		queue_redraw()

	func _font() -> Font:
		return get_theme_default_font()

	func _texts(i: int) -> PackedStringArray:
		var n: Dictionary = g["nodes"][i]
		var tokens := c[i] if i < c.size() else 0
		return PackedStringArray([String(n["name"]), "%s: %d" % [n["label"], tokens]])

	func _layout() -> void:
		boxes.clear()
		content = Vector2.ZERO
		var nodes: Array = g.get("nodes", [])
		if nodes.is_empty():
			custom_minimum_size = Vector2.ZERO
			return
		var font := _font()
		var w := 0.0
		var low := INF
		for i in nodes.size():
			var n: Dictionary = nodes[i]
			w = maxf(w, font.get_string_size(String(n["name"]), HORIZONTAL_ALIGNMENT_LEFT, -1, NAME_SIZE).x)
			# Room for counts of up to 4 digits.
			w = maxf(w, font.get_string_size("%s: 0000" % n["label"], HORIZONTAL_ALIGNMENT_LEFT, -1, LABEL_SIZE).x)
			low = minf(low, float(n.get("lane", 0.0)))
		var box := Vector2(ceilf(w + PAD * 2.0), NAME_SIZE + LABEL_SIZE + PAD * 2.5)
		for n in nodes:
			var at := Vector2(PAD + float(n.get("layer", 0)) * (box.x + GAP.x), PAD + (float(n.get("lane", 0.0)) - low) * (box.y + GAP.y))
			boxes.append(Rect2(at, box))
			content = content.max(at + box + Vector2(PAD, PAD + GAP.y))
		_resize()

	func _resize() -> void:
		custom_minimum_size = content * zoom
		queue_redraw()

	func set_zoom(z: float) -> void:
		zoom = clampf(z, 0.3, 2.5)
		_resize()

	## Fits the width of the scroll area (`MIN_FIT` at least, 1 at most).
	func fit() -> void:
		var room := get_parent_control().size.x if get_parent_control() != null else 0.0
		if room <= 0.0 or content.x <= 0.0:
			set_zoom(1.0)
			return
		set_zoom(clampf((room - 4.0) / content.x, MIN_FIT, 1.0))

	func _gui_input(e: InputEvent) -> void:
		var mb := e as InputEventMouseButton
		if mb != null and mb.pressed and mb.ctrl_pressed and mb.button_index in [MOUSE_BUTTON_WHEEL_UP, MOUSE_BUTTON_WHEEL_DOWN]:
			set_zoom(zoom * (1.1 if mb.button_index == MOUSE_BUTTON_WHEEL_UP else 1.0 / 1.1))
			accept_event()

	## Pairs of boxes that overlap (scripted checks: 0).
	func overlaps() -> int:
		var n := 0
		for i in boxes.size():
			for j in range(i + 1, boxes.size()):
				if boxes[i].intersects(boxes[j]):
					n += 1
		return n

	## Activities whose text does not fit their box (scripted checks: 0).
	func clipped() -> int:
		var n := 0
		var font := _font()
		for i in boxes.size():
			var t := _texts(i)
			var inner := boxes[i].size.x - PAD * 2.0 + 0.5
			if font.get_string_size(t[0], HORIZONTAL_ALIGNMENT_LEFT, -1, NAME_SIZE).x > inner or font.get_string_size(t[1], HORIZONTAL_ALIGNMENT_LEFT, -1, LABEL_SIZE).x > inner:
				n += 1
		return n

	func _draw() -> void:
		draw_set_transform(Vector2.ZERO, 0.0, Vector2(zoom, zoom))
		draw_rect(Rect2(Vector2.ZERO, size / zoom), Color(0.09, 0.10, 0.12))
		var nodes: Array = g.get("nodes", [])
		if nodes.is_empty() or boxes.size() != nodes.size():
			return
		var font := _font()
		var edges: PackedInt32Array = g.get("edges", PackedInt32Array())
		var arrow := Color(0.55, 0.60, 0.68)
		for k in range(0, edges.size() - 1, 2):
			var ra: Rect2 = boxes[edges[k]]
			var rb: Rect2 = boxes[edges[k + 1]]
			var a := ra.get_center()
			var b := rb.get_center()
			if rb.position.x > ra.end.x:
				# Forward: from the right side of one box to the left side of the next.
				a = Vector2(ra.end.x, a.y)
				b = Vector2(rb.position.x, b.y)
			else:
				# Back (a rework loop) or in the same column: under the boxes.
				a = Vector2(a.x, ra.end.y)
				b = Vector2(b.x, rb.end.y)
				var low := maxf(ra.end.y, rb.end.y) + GAP.y * 0.6
				draw_polyline(PackedVector2Array([a, Vector2(a.x, low), Vector2(b.x, low)]), arrow, 1.5, true)
				a = Vector2(b.x, low)
			draw_line(a, b, arrow, 1.5, true)
			var d := (b - a).normalized()
			var s := Vector2(-d.y, d.x) * 4.0
			draw_colored_polygon(PackedVector2Array([b, b - d * 9 + s, b - d * 9 - s]), arrow)
		for i in nodes.size():
			var r: Rect2 = boxes[i]
			var tokens := c[i] if i < c.size() else 0
			var t := _texts(i)
			draw_rect(r, Color(0.16, 0.18, 0.22))
			draw_rect(r, Color(0.98, 0.80, 0.35) if tokens > 0 else Color(0.40, 0.45, 0.52), false, 2.0 if tokens > 0 else 1.0)
			draw_string(font, r.position + Vector2(PAD, PAD + NAME_SIZE * 0.9), t[0], HORIZONTAL_ALIGNMENT_LEFT, -1, NAME_SIZE, Color(0.88, 0.90, 0.95))
			draw_string(font, r.position + Vector2(PAD, PAD * 1.5 + NAME_SIZE + LABEL_SIZE * 0.9), t[1], HORIZONTAL_ALIGNMENT_LEFT, -1, LABEL_SIZE, Color(0.98, 0.80, 0.35) if tokens > 0 else Color(0.62, 0.66, 0.72))
