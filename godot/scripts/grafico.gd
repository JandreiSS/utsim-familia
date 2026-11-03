## A small chart drawn with `draw_*` (phase G3): the Godot side has no plotting library,
## so the results panel, the PERT window and the token flows draw with this one control.
## The numbers come ready from the bridge; here they are only scaled and drawn.
##
## Four kinds, set with one call each (the last call wins):
## - `bars(rows, unit, max)`: horizontal bars with the 95% whisker (`{label, value,
##   half_width, hot}`), the hot one orange (the bottleneck);
## - `stacked(rows)`: one bar per row split in coloured pieces (`{label, segments:
##   [{fraction, color, label}]}`): the states, with the colours of the 3D view;
## - `lines(series, opts)`: curves over a common x axis (`{points: [[x, y]...], color,
##   width}`), optional vertical markers (`opts.markers: [{x, color, label}]`) and axis
##   names (`opts.x_label`, `opts.y_label`);
## - `histogram(h, marker, label)`: the counts of `{min, bin_width, counts}`, with an
##   optional vertical line (the deadline).
class_name Grafico
extends Control

const AXIS := Color(0.55, 0.60, 0.68)
const GRID := Color(0.22, 0.24, 0.29)
const TEXT := Color(0.86, 0.89, 0.94)
const BAR := Color(0.35, 0.59, 0.94)
const HOT := Color(1.0, 0.59, 0.24)
const MARGIN_LEFT := 150.0
const FONT_SIZE := 13

var kind := ""
var data: Variant = null
var opts: Dictionary = {}
## Short text drawn when there is nothing to show.
var empty_text := "sem dados"


func _ready() -> void:
	if custom_minimum_size == Vector2.ZERO:
		custom_minimum_size = Vector2(360, 220)
	clip_contents = true


func bars(rows: Array, unit := "", max_value := 0.0) -> void:
	kind = "barras"
	data = rows
	opts = {"unit": unit, "max": max_value}
	custom_minimum_size.y = maxf(120.0, 26.0 * rows.size() + 40.0)
	queue_redraw()


func stacked(rows: Array) -> void:
	kind = "estados"
	data = rows
	opts = {}
	custom_minimum_size.y = maxf(120.0, 26.0 * rows.size() + 70.0)
	queue_redraw()


func lines(series: Array, options := {}) -> void:
	kind = "linhas"
	data = series
	opts = options
	queue_redraw()


func histogram(h: Variant, marker := NAN, marker_label := "") -> void:
	kind = "histograma"
	data = h
	opts = {"marker": marker, "label": marker_label}
	queue_redraw()


func clear() -> void:
	kind = ""
	data = null
	queue_redraw()


## How many things are drawn (bars, rows, curves or bins): scripted checks.
func count() -> int:
	match kind:
		"barras", "estados", "linhas":
			return (data as Array).size()
		"histograma":
			return (data["counts"] as Array).size() if data is Dictionary else 0
	return 0


func _font() -> Font:
	return ThemeDB.fallback_font


func _text(at: Vector2, s: String, color := TEXT, align := HORIZONTAL_ALIGNMENT_LEFT, width := -1.0) -> void:
	draw_string(_font(), at, s, align, width, FONT_SIZE, color)


static func short(x: float) -> String:
	if is_nan(x):
		return "-"
	var a := absf(x)
	if a >= 1000.0:
		return String.num(x, 0)
	if a >= 10.0:
		return String.num(x, 1)
	if a >= 1.0:
		return String.num(x, 2)
	return String.num(x, 3)


func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), Color(0.09, 0.10, 0.12))
	var empty := data == null or (data is Array and (data as Array).is_empty())
	if kind == "" or empty:
		_text(Vector2(12, 24), empty_text, AXIS)
		return
	match kind:
		"barras":
			_draw_bars()
		"estados":
			_draw_stacked()
		"linhas":
			_draw_lines()
		"histograma":
			_draw_histogram()


func _draw_bars() -> void:
	var rows: Array = data
	var top := 14.0
	var right := size.x - 60.0
	var hi := float(opts.get("max", 0.0))
	for r in rows:
		var v = r.get("value")
		var h = r.get("half_width")
		var top_v := (float(v) if v != null else 0.0) + (float(h) if h != null else 0.0)
		hi = maxf(hi, top_v)
	if hi <= 0.0:
		hi = 1.0
	var sx := (right - MARGIN_LEFT) / hi
	var unit: String = opts.get("unit", "")
	for k in rows.size():
		var r: Dictionary = rows[k]
		var y := top + k * 26.0
		_text(Vector2(8, y + 15), String(r.get("label", "")), TEXT, HORIZONTAL_ALIGNMENT_LEFT, MARGIN_LEFT - 12)
		var v = r.get("value")
		if v == null:
			continue
		var w := float(v) * sx
		draw_rect(Rect2(MARGIN_LEFT, y + 3, maxf(w, 1.0), 16), HOT if r.get("hot", false) else BAR)
		var h = r.get("half_width")
		if h != null and float(h) > 0.0:
			var a := MARGIN_LEFT + (float(v) - float(h)) * sx
			var b := MARGIN_LEFT + (float(v) + float(h)) * sx
			draw_line(Vector2(a, y + 11), Vector2(b, y + 11), TEXT, 1.5)
			draw_line(Vector2(a, y + 6), Vector2(a, y + 16), TEXT, 1.5)
			draw_line(Vector2(b, y + 6), Vector2(b, y + 16), TEXT, 1.5)
		var label := short(float(v)) + unit
		if h != null:
			label = "%s ± %s%s" % [short(float(v)), short(float(h)), unit]
		_text(Vector2(MARGIN_LEFT + w + 6, y + 15), label, TEXT)
	draw_line(Vector2(MARGIN_LEFT, top - 4), Vector2(MARGIN_LEFT, top + rows.size() * 26.0), AXIS)


func _draw_stacked() -> void:
	var rows: Array = data
	var top := 14.0
	var right := size.x - 16.0
	var w := right - MARGIN_LEFT
	var legend := {}
	for k in rows.size():
		var r: Dictionary = rows[k]
		var y := top + k * 26.0
		_text(Vector2(8, y + 15), String(r.get("label", "")), TEXT, HORIZONTAL_ALIGNMENT_LEFT, MARGIN_LEFT - 12)
		var x := MARGIN_LEFT
		for s in r.get("segments", []):
			var f := float(s.get("fraction", 0.0)) if s.get("fraction") != null else 0.0
			var c: Variant = s.get("color", [0.6, 0.6, 0.6])
			var col := Color(float(c[0]), float(c[1]), float(c[2]))
			draw_rect(Rect2(x, y + 3, f * w, 18), col)
			if f * w > 40:
				_text(Vector2(x + 3, y + 17), "%d%%" % roundi(f * 100.0), Color(0.05, 0.05, 0.06))
			x += f * w
			legend[String(s.get("label", ""))] = col
	var ly := top + rows.size() * 26.0 + 16.0
	var lx := 8.0
	for name in legend.keys():
		draw_rect(Rect2(lx, ly - 10, 12, 12), legend[name])
		_text(Vector2(lx + 16, ly), name, TEXT)
		lx += 30.0 + _font().get_string_size(name, HORIZONTAL_ALIGNMENT_LEFT, -1, FONT_SIZE).x
		if lx > size.x - 120:
			lx = 8.0
			ly += 18.0


func _bounds(series: Array) -> Rect2:
	var x0 := INF
	var x1 := -INF
	var y0 := 0.0
	var y1 := -INF
	for s in series:
		for p in s.get("points", []):
			var x := float(p[0])
			var y := float(p[1])
			x0 = minf(x0, x)
			x1 = maxf(x1, x)
			y0 = minf(y0, y)
			y1 = maxf(y1, y)
	if x0 == INF:
		return Rect2(0, 0, 1, 1)
	if x1 <= x0:
		x1 = x0 + 1.0
	if y1 <= y0:
		y1 = y0 + 1.0
	return Rect2(x0, y0, x1 - x0, (y1 - y0) * 1.05)


func _axes(plot: Rect2, b: Rect2) -> void:
	draw_line(plot.position + Vector2(0, plot.size.y), plot.end, AXIS)
	draw_line(plot.position, plot.position + Vector2(0, plot.size.y), AXIS)
	for k in 5:
		var f := k / 4.0
		var y := plot.end.y - f * plot.size.y
		draw_line(Vector2(plot.position.x, y), Vector2(plot.end.x, y), GRID)
		_text(Vector2(4, y + 4), short(b.position.y + f * b.size.y), AXIS)
		var x := plot.position.x + f * plot.size.x
		_text(Vector2(x - 20, plot.end.y + 16), short(b.position.x + f * b.size.x), AXIS)


func _draw_lines() -> void:
	var series: Array = data
	var b := _bounds(series)
	var plot := Rect2(56, 12, size.x - 70, size.y - 44)
	_axes(plot, b)
	var to_screen := func(x: float, y: float) -> Vector2:
		return Vector2(plot.position.x + (x - b.position.x) / b.size.x * plot.size.x,
			plot.end.y - (y - b.position.y) / b.size.y * plot.size.y)
	for s in series:
		var pts := PackedVector2Array()
		for p in s.get("points", []):
			pts.append(to_screen.call(float(p[0]), float(p[1])))
		if pts.size() >= 2:
			draw_polyline(pts, s.get("color", BAR), float(s.get("width", 1.5)), true)
		elif pts.size() == 1:
			draw_circle(pts[0], 2.5, s.get("color", BAR))
	for m in opts.get("markers", []):
		var x: float = float(m.get("x", NAN))
		if is_nan(x):
			continue
		var at: Vector2 = to_screen.call(x, b.position.y)
		var col: Color = m.get("color", HOT)
		draw_line(Vector2(at.x, plot.position.y), Vector2(at.x, plot.end.y), col, 1.5)
		_text(Vector2(at.x + 4, plot.position.y + 14), String(m.get("label", "")), col)
	if opts.has("x_label"):
		_text(Vector2(plot.end.x - 120, size.y - 4), String(opts["x_label"]), AXIS, HORIZONTAL_ALIGNMENT_RIGHT, 120)
	if opts.has("y_label"):
		_text(Vector2(60, 12), String(opts["y_label"]), AXIS)


func _draw_histogram() -> void:
	var h: Dictionary = data
	var counts: Array = h.get("counts", [])
	if counts.is_empty():
		_text(Vector2(12, 24), empty_text, AXIS)
		return
	var lo := float(h.get("min", 0.0)) if h.get("min") != null else 0.0
	var width := float(h.get("bin_width", 1.0)) if h.get("bin_width") != null else 1.0
	if width <= 0.0:
		width = 1.0
	var hi := lo + width * counts.size()
	var top := 1.0
	for c in counts:
		top = maxf(top, float(c))
	var plot := Rect2(56, 12, size.x - 70, size.y - 44)
	_axes(plot, Rect2(lo, 0, hi - lo, top * 1.05))
	var bw := plot.size.x / counts.size()
	for k in counts.size():
		var f := float(counts[k]) / (top * 1.05)
		var r := Rect2(plot.position.x + k * bw + 1, plot.end.y - f * plot.size.y, maxf(bw - 2, 1), f * plot.size.y)
		draw_rect(r, BAR)
	var marker := float(opts.get("marker", NAN))
	if not is_nan(marker) and marker >= lo and marker <= hi:
		var x := plot.position.x + (marker - lo) / (hi - lo) * plot.size.x
		draw_line(Vector2(x, plot.position.y), Vector2(x, plot.end.y), HOT, 2.0)
		_text(Vector2(x + 4, plot.position.y + 14), String(opts.get("label", "")), HOT)
