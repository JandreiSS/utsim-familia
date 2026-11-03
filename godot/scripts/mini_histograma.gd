## A small histogram of real data with the density of the chosen family drawn over it
## (the panel of the data of a field). `set_data` takes the `chart` of the fit view
## (`UtsimDocument.data_fields()[..].view.chart`): `{lo, hi, density, curve}`.
## Colours come from the theme (the section colour for the bars, the warning colour for
## the curve), so nothing here is styled by hand.
class_name MiniHistograma
extends Control

var _chart := {}


func _init() -> void:
	custom_minimum_size = Vector2(0, 120)
	size_flags_horizontal = Control.SIZE_EXPAND_FILL
	mouse_filter = Control.MOUSE_FILTER_PASS


func set_data(chart: Dictionary) -> void:
	_chart = chart
	queue_redraw()


## Number of bars drawn (scripted checks).
func bars() -> int:
	return (_chart.get("density", []) as Array).size()


## Number of points of the curve (0 for the empirical distribution).
func curve_points() -> int:
	return (_chart.get("curve", []) as Array).size()


func _draw() -> void:
	var bar_color := get_theme_color("font_color", "TituloSecao")
	bar_color.a = 0.55
	var curve_color := get_theme_color("font_color", "AvisoDados")
	var text_color := get_theme_color("font_color", "Ajuda")
	var font := get_theme_default_font()
	var margin_bottom := 16.0
	var area := Rect2(2, 2, size.x - 4, size.y - margin_bottom - 4)
	draw_rect(area, Color(0, 0, 0, 0.18))
	var density: Array = _chart.get("density", [])
	if density.is_empty():
		return
	var curve: Array = _chart.get("curve", [])
	var top := 0.0
	for d in density:
		top = maxf(top, float(d))
	for p in curve:
		top = maxf(top, float(p[1]))
	if top <= 0.0:
		return
	top *= 1.08
	var n := density.size()
	var w := area.size.x / n
	for k in n:
		var h := area.size.y * float(density[k]) / top
		draw_rect(Rect2(area.position.x + k * w + 1, area.end.y - h, w - 2, h), bar_color)
	var lo := float(_chart.get("lo", 0.0))
	var hi := float(_chart.get("hi", 1.0))
	var span := maxf(hi - lo, 1e-12)
	if curve.size() > 1:
		var pts := PackedVector2Array()
		for p in curve:
			var x := area.position.x + area.size.x * (float(p[0]) - lo) / span
			var y := area.end.y - area.size.y * minf(float(p[1]) / top, 1.0)
			pts.append(Vector2(x, y))
		draw_polyline(pts, curve_color, 2.0, true)
	var fs := 11
	draw_string(font, Vector2(2, size.y - 3), PainelPropriedades._num(lo), HORIZONTAL_ALIGNMENT_LEFT, -1, fs, text_color)
	var right := PainelPropriedades._num(hi)
	var width := font.get_string_size(right, HORIZONTAL_ALIGNMENT_LEFT, -1, fs).x
	draw_string(font, Vector2(size.x - width - 2, size.y - 3), right, HORIZONTAL_ALIGNMENT_LEFT, -1, fs, text_color)
