## The properties panel of Editar, drawn from the schema of `UtsimDocument.properties()`:
## one row per field, by section, with a widget per field type (see
## `crates/utsim-godot/src/schema.rs`). The panel never changes the model: it emits
## `edited(path, value)` and the main scene applies it through the document (undo,
## validation) and calls `show_properties` again.
##
## Problems of the object are listed at the top in red, warnings in yellow. The outputs
## are listed in port order with buttons to reorder or remove them.
##
## Widgets are named `Prop_<path>` (dots as underscores) so scripts can find them; a
## distribution adds `_familia`, `_<parameter>`, `_texto` (the toggle) and `_ron`.
class_name PainelPropriedades
extends VBoxContainer

signal edited(path: String, value: Variant)
## The end of a continuous gesture (the colour picker closed).
signal gesture_ended
signal reorder(index: int, delta: int)
signal disconnect_output(to: String)
signal action(path: String)

const RED := Color(1.0, 0.45, 0.42)
const YELLOW := Color(0.98, 0.80, 0.35)
const SECTION := Color(0.62, 0.76, 0.98)
const LABEL_WIDTH := 170

var families := PackedStringArray()
var _as_text := {}
var _asset_filter := ""
var _props: Dictionary = {}
var _extra := ""


static func widget_name(path: String) -> String:
	return "Prop_" + path.replace(".", "_")


func clear() -> void:
	for c in get_children():
		remove_child(c)
		c.queue_free()


## Draws `props` (`UtsimDocument.properties`). `extra`: a line under the title (several
## objects selected).
func show_properties(props: Dictionary, extra := "") -> void:
	_props = props
	_extra = extra
	clear()
	var title := Label.new()
	title.name = "Titulo"
	title.add_theme_font_size_override("font_size", 16)
	title.text = "Propriedades: %s" % props.get("name", "")
	if props.get("kind", "") == "run":
		title.text = "Modelo (nada selecionado)"
	add_child(title)
	var sub := Label.new()
	sub.text = props.get("kind_label", "")
	sub.add_theme_color_override("font_color", Color(0.65, 0.70, 0.78))
	add_child(sub)
	if extra != "":
		var e := _note(extra, YELLOW)
		add_child(e)
		_arrange_buttons()
	_issues("Problemas", props.get("problems", PackedStringArray()), RED, "Problemas")
	_issues("Avisos", props.get("warnings", PackedStringArray()), YELLOW, "Avisos")
	var section := ""
	for f in props.get("fields", []):
		var field: Dictionary = f
		if field["section"] != section:
			section = field["section"]
			_section(section)
		_field(field)
		# The port order goes right after the output rule (mobile resources have none).
		if field["path"] == "output":
			_outputs(props)


## Align and distribute the selection (shown when several objects are selected).
func _arrange_buttons() -> void:
	var rows := [
		["Alinhar", [["esquerda", "⇤ esq."], ["centro_x", "centro X"], ["direita", "dir. ⇥"]]],
		["", [["topo", "⤒ topo"], ["centro_y", "centro Y"], ["base", "base ⤓"]]],
		["Distribuir", [["x", "em X ↔"], ["y", "em Y ↕"]]],
	]
	for r in rows:
		var row := HBoxContainer.new()
		var l := Label.new()
		l.text = r[0]
		l.custom_minimum_size = Vector2(80, 0)
		row.add_child(l)
		for b in r[1]:
			var what: String = ("distribuir:" if r[0] == "Distribuir" else "alinhar:") + String(b[0])
			var button := Button.new()
			button.name = ("Distribuir_" if r[0] == "Distribuir" else "Alinhar_") + String(b[0])
			button.text = b[1]
			button.focus_mode = Control.FOCUS_NONE
			button.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			button.pressed.connect(func(): action.emit(what))
			row.add_child(button)
		add_child(row)


func _note(text: String, color: Color) -> Label:
	var l := Label.new()
	l.text = text
	l.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	l.add_theme_color_override("font_color", color)
	l.custom_minimum_size = Vector2(300, 0)
	return l


func _issues(title: String, list: PackedStringArray, color: Color, node_name: String) -> void:
	if list.is_empty():
		return
	var box := VBoxContainer.new()
	box.name = node_name
	var head := Label.new()
	head.text = "%s (%d)" % [title, list.size()]
	head.add_theme_color_override("font_color", color)
	head.add_theme_font_size_override("font_size", 14)
	box.add_child(head)
	for m in list:
		box.add_child(_note("• " + m, color))
	add_child(box)


func _section(text: String) -> void:
	add_child(HSeparator.new())
	var l := Label.new()
	l.text = text
	l.add_theme_color_override("font_color", SECTION)
	l.add_theme_font_size_override("font_size", 14)
	add_child(l)


func _row(field: Dictionary) -> HBoxContainer:
	var row := HBoxContainer.new()
	var label := Label.new()
	label.text = field["label"]
	label.custom_minimum_size = Vector2(LABEL_WIDTH - 14 * int(field["depth"]), 0)
	label.clip_text = true
	label.tooltip_text = field["help"]
	label.mouse_filter = Control.MOUSE_FILTER_PASS
	if int(field["depth"]) > 0:
		var pad := Control.new()
		pad.custom_minimum_size = Vector2(14 * int(field["depth"]), 0)
		row.add_child(pad)
	row.add_child(label)
	add_child(row)
	return row


func _toggle(row: HBoxContainer, field: Dictionary) -> CheckBox:
	var c := CheckBox.new()
	c.name = widget_name(field["path"]) + "_ligado"
	c.button_pressed = field["enabled"]
	c.tooltip_text = "Liga ou desliga " + String(field["label"]).to_lower()
	var path: String = field["path"]
	c.toggled.connect(func(on): edited.emit(path + ".on", on))
	row.add_child(c)
	return c


func _field(field: Dictionary) -> void:
	var path: String = field["path"]
	var ty: String = field["type"]
	match ty:
		"group":
			var row := _row(field)
			(row.get_child(row.get_child_count() - 1) as Label).add_theme_color_override("font_color", Color(0.85, 0.88, 0.94))
			if field["optional"]:
				_toggle(row, field)
		"info":
			var row := _row(field)
			var v := Label.new()
			v.text = str(field["value"])
			v.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
			v.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			row.add_child(v)
		"text":
			var row := _row(field)
			var e := _line(path, str(field["value"]))
			row.add_child(e)
		"script":
			var row := _row(field)
			if field["optional"]:
				_toggle(row, field)
			if field["value"] == null:
				return
			var t := TextEdit.new()
			t.name = widget_name(path)
			t.text = str(field["value"])
			t.custom_minimum_size = Vector2(0, 64)
			t.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			add_child(t)
			var apply := Button.new()
			apply.text = "Aplicar script"
			apply.pressed.connect(func(): edited.emit(path, t.text))
			add_child(apply)
		"float", "int":
			var row := _row(field)
			if field["optional"]:
				_toggle(row, field)
			if field["value"] == null:
				return
			row.add_child(_spin(field))
		"bool":
			var row := _row(field)
			var c := CheckBox.new()
			c.name = widget_name(path)
			c.button_pressed = bool(field["value"])
			c.tooltip_text = field["help"]
			c.toggled.connect(func(on): edited.emit(path, on))
			row.add_child(c)
		"choice":
			var row := _row(field)
			var o := OptionButton.new()
			o.name = widget_name(path)
			o.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			o.clip_text = true
			o.tooltip_text = field["help"]
			var values := []
			for c in field["choices"]:
				o.add_item(c["label"])
				values.append(c["value"])
			o.selected = values.find(field["value"])
			o.item_selected.connect(func(k): edited.emit(path, values[k]))
			row.add_child(o)
		"ron":
			var row := _row(field)
			var e := _line(path, str(field["value"]))
			e.tooltip_text = field["help"]
			e.add_theme_font_override("font", _mono())
			row.add_child(e)
		"set":
			var row := _row(field)
			var box := VBoxContainer.new()
			box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			var members: PackedStringArray = field["value"]
			for c in field["choices"]:
				var cb := CheckBox.new()
				var member: String = c["value"]
				cb.name = widget_name(path + "." + member)
				cb.text = c["label"]
				cb.button_pressed = members.has(member)
				cb.toggled.connect(func(on): edited.emit(path + "." + member, on))
				box.add_child(cb)
			if field["choices"].is_empty():
				box.add_child(_note("Nenhum transportador no modelo.", Color(0.65, 0.70, 0.78)))
			row.add_child(box)
		"dist":
			_dist(field)
		"color":
			var row := _row(field)
			_toggle(row, field)
			if field["value"] == null:
				return
			var p := ColorPickerButton.new()
			p.name = widget_name(path)
			p.color = Color.from_string(str(field["value"]), Color.WHITE)
			p.edit_alpha = false
			p.custom_minimum_size = Vector2(60, 0)
			p.color_changed.connect(func(c: Color): edited.emit(path, "#" + c.to_html(false)))
			p.popup_closed.connect(func(): gesture_ended.emit())
			row.add_child(p)
			var hex := Label.new()
			hex.text = str(field["value"])
			row.add_child(hex)
		"asset":
			_asset(field)
		"action":
			var b := Button.new()
			b.name = widget_name(path)
			b.text = field["label"]
			b.tooltip_text = field["help"]
			b.disabled = not bool(field["value"])
			b.pressed.connect(func(): edited.emit(path, null))
			add_child(b)


func _mono() -> Font:
	var f := SystemFont.new()
	f.font_names = PackedStringArray(["Consolas", "DejaVu Sans Mono", "monospace"])
	return f


## A text field committed on Enter or when it loses focus (only when changed).
func _line(path: String, value: String) -> LineEdit:
	var e := LineEdit.new()
	e.name = widget_name(path)
	e.text = value
	e.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	e.select_all_on_focus = true
	var commit := func(_t = null):
		if e.text != value:
			edited.emit(path, e.text)
	e.text_submitted.connect(commit)
	e.focus_exited.connect(commit)
	return e


func _spin(field: Dictionary, path_override := "", value_override: Variant = null) -> SpinBox:
	var path: String = field["path"] if path_override == "" else path_override
	var integer: bool = field["type"] == "int"
	var value: float = float(field["value"] if value_override == null else value_override)
	var s := SpinBox.new()
	s.name = widget_name(path)
	s.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	# A small range keeps the box narrow (its width follows the longest bound); values
	# beyond it are still accepted.
	s.allow_greater = true
	s.allow_lesser = field.get("min") == null
	s.min_value = float(field["min"]) if field.get("min") != null else -999.0
	s.max_value = 999.0
	s.step = 1.0 if integer else 0.001
	s.custom_arrow_step = 1.0 if integer else 0.1
	s.rounded = integer
	s.select_all_on_focus = true
	s.suffix = field.get("unit", "")
	s.tooltip_text = field.get("help", "")
	s.value = value
	s.value_changed.connect(func(v: float):
		edited.emit(path, int(v) if integer else v))
	return s


## A distribution: family, parameters and the mean, or its RON text ("editar como
## texto"), which is also the only way for tables (Discreta, Empírica).
func _dist(field: Dictionary) -> void:
	var path: String = field["path"]
	var row := _row(field)
	if field["optional"]:
		_toggle(row, field)
	var view: Dictionary = field.get("dist", {})
	if view.is_empty():
		return
	var family := OptionButton.new()
	family.name = widget_name(path) + "_familia"
	family.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	for k in families.size():
		family.add_item(families[k], k)
	family.selected = int(view["family"])
	family.item_selected.connect(func(k): edited.emit(path + ".family", k))
	row.add_child(family)
	var text_on: bool = _as_text.get(path, false) or (view["params"] as Array).is_empty()
	var toggle := Button.new()
	toggle.name = widget_name(path) + "_texto"
	toggle.text = "{ }"
	toggle.toggle_mode = true
	toggle.button_pressed = text_on
	toggle.tooltip_text = "Editar como texto (RON)"
	toggle.toggled.connect(func(on):
		_as_text[path] = on
		show_properties(_props, _extra))
	row.add_child(toggle)
	var box := VBoxContainer.new()
	add_child(box)
	var indent := 14 * (int(field["depth"]) + 1)
	if text_on:
		var e := _line(path, view["text"])
		e.name = widget_name(path) + "_ron"
		e.add_theme_font_override("font", _mono())
		var h := HBoxContainer.new()
		var pad := Control.new()
		pad.custom_minimum_size = Vector2(indent, 0)
		h.add_child(pad)
		h.add_child(e)
		box.add_child(h)
	else:
		for p in view["params"]:
			var h := HBoxContainer.new()
			var pad := Control.new()
			pad.custom_minimum_size = Vector2(indent, 0)
			h.add_child(pad)
			var l := Label.new()
			l.text = p["label"]
			l.custom_minimum_size = Vector2(LABEL_WIDTH - indent, 0)
			h.add_child(l)
			var fake := {"type": "int" if p["integer"] else "float", "value": p["value"], "min": null, "unit": "", "help": ""}
			h.add_child(_spin(fake, path + "." + String(p["key"])))
			box.add_child(h)
	var mean := Label.new()
	mean.text = "média %s" % _num(float(view["mean"]))
	mean.add_theme_color_override("font_color", Color(0.65, 0.70, 0.78))
	mean.add_theme_font_size_override("font_size", 12)
	var mh := HBoxContainer.new()
	var mpad := Control.new()
	mpad.custom_minimum_size = Vector2(indent, 0)
	mh.add_child(mpad)
	mh.add_child(mean)
	box.add_child(mh)


## The asset of the look: a search box and the library and catalogue entries, with the
## credits as tooltips.
func _asset(field: Dictionary) -> void:
	var path: String = field["path"]
	var row := _row(field)
	var current: String = field["value"]
	var o := OptionButton.new()
	o.name = widget_name(path)
	o.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	o.clip_text = true
	var values := []
	var group := ""
	var wanted := _asset_filter.strip_edges().to_lower()
	for c in field["choices"]:
		var v: String = c["value"]
		if wanted != "" and v != current and not String(c["label"]).to_lower().contains(wanted):
			continue
		if c["group"] != group:
			group = c["group"]
			o.add_separator(group)
			values.append(null)
		o.add_item(c["label"])
		o.set_item_tooltip(o.item_count - 1, c["help"])
		values.append(v)
	o.selected = values.find(current)
	o.item_selected.connect(func(k):
		if values[k] != null:
			edited.emit(path, values[k]))
	row.add_child(o)
	var search_row := HBoxContainer.new()
	var pad := Control.new()
	pad.custom_minimum_size = Vector2(LABEL_WIDTH, 0)
	search_row.add_child(pad)
	var search := LineEdit.new()
	search.name = widget_name(path) + "_busca"
	search.placeholder_text = "Buscar ativo"
	search.text = _asset_filter
	search.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	search.text_submitted.connect(func(t):
		_asset_filter = t
		show_properties(_props, _extra))
	search_row.add_child(search)
	add_child(search_row)
	if field["help"] != "":
		add_child(_note(field["help"], Color(0.65, 0.70, 0.78)))


## Outputs in port order, with ▲ ▼ to reorder and ✕ to remove; then the inputs.
func _outputs(props: Dictionary) -> void:
	var outs: PackedStringArray = props.get("outputs", PackedStringArray())
	var ins: PackedStringArray = props.get("inputs", PackedStringArray())
	var head := Label.new()
	head.text = "Ligações de saída (ordem das portas)"
	head.add_theme_color_override("font_color", Color(0.85, 0.88, 0.94))
	add_child(head)
	if outs.is_empty():
		add_child(_note("Nenhuma. Use a ferramenta Ligar (L).", Color(0.65, 0.70, 0.78)))
	for k in outs.size():
		var row := HBoxContainer.new()
		row.name = "Saida_%d" % (k + 1)
		var l := Label.new()
		l.text = "%d → %s" % [k + 1, outs[k]]
		l.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		l.clip_text = true
		row.add_child(l)
		var up := Button.new()
		up.name = "Subir"
		up.text = "▲"
		up.tooltip_text = "Sobe esta saída na ordem das portas"
		up.disabled = k == 0
		up.pressed.connect(func(): reorder.emit(k, -1))
		row.add_child(up)
		var down := Button.new()
		down.name = "Descer"
		down.text = "▼"
		down.tooltip_text = "Desce esta saída na ordem das portas"
		down.disabled = k == outs.size() - 1
		down.pressed.connect(func(): reorder.emit(k, 1))
		row.add_child(down)
		var rm := Button.new()
		rm.name = "Remover"
		rm.text = "✕"
		rm.tooltip_text = "Remove esta ligação"
		var to := outs[k]
		rm.pressed.connect(func(): disconnect_output.emit(to))
		row.add_child(rm)
		add_child(row)
	if not ins.is_empty():
		add_child(_note("Entradas: " + ", ".join(ins), Color(0.65, 0.70, 0.78)))


static func _num(x: float) -> String:
	if x == 0.0 or is_nan(x) or is_inf(x):
		return str(x)
	var digits := clampi(3 - int(floor(log(absf(x)) / log(10.0))), 0, 6)
	return String.num(x, digits)
