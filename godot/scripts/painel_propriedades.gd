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
##
## The asset of the look has a miniature (`Prop_visual_asset_miniatura`): the object
## alone, drawn by the same scene code in its own small 3D world, with the asset chosen,
## or with the one under the mouse while the list is open. `look_provider(asset)` gives
## its scene (`UtsimDocument.look_scene`).
class_name PainelPropriedades
extends VBoxContainer

signal edited(path: String, value: Variant)
## The end of a continuous gesture (the colour picker closed).
signal gesture_ended
signal reorder(index: int, delta: int)
signal disconnect_output(to: String)
signal action(path: String)
## A problem or warning of the list was clicked: the main scene outlines what it names.
signal issue_clicked(message: String)
## Real data in a distribution field (`dados_reais.gd`): `path` is the field inside the
## selected object (`process`). `draft` holds the choices of the form.
signal data_apply(path: String, draft: Dictionary)
## The applied entry has no analysis yet (a file opened with data, an undo): fit it.
signal data_analyze(path: String, draft: Dictionary)
signal data_to_dist(path: String)
signal data_report
signal data_pick_file(path: String)

const RED := Color(1.0, 0.45, 0.42)
const YELLOW := Color(0.98, 0.80, 0.35)
## Grey of the short help lines (the theme's "Ajuda").
const HELP := Color(0.65, 0.70, 0.78)
const LABEL_WIDTH := 170

var families := PackedStringArray()
## Names of the families a field can be fixed to (`UtsimDocument.data_families`).
var data_families := PackedStringArray()
## `func(file: String) -> Array`: the columns of a data file.
var columns_provider := Callable()
## Drafts of the "Dados reais" forms by `object|path`, and the entry each was made from.
var drafts := {}
var drafts_seen := {}
var _data_mode := {}
var _requested := {}
## `func(asset: String) -> UtsimPlayback` (or null): the scene of the selected object
## drawn with `asset`, for the miniature.
var look_provider := Callable()
var _mini_view: SubViewport
var _mini_scene: CenaUtsim
var _mini_camera: Camera3D
var _mini_asset := "-"
var _as_text := {}
var _asset_filter := ""
var _props: Dictionary = {}
var _extra := ""
## Sections the user closed (by title); they stay closed for every object.
var _closed := {}


static func widget_name(path: String) -> String:
	return "Prop_" + path.replace(".", "_")


func clear() -> void:
	_mini_scene = null
	_mini_view = null
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
	title.theme_type_variation = &"TituloPainel"
	title.text = "Propriedades: %s" % props.get("name", "")
	if props.get("kind", "") == "run":
		title.text = "Modelo (nada selecionado)"
	add_child(title)
	var sub := Label.new()
	sub.text = props.get("kind_label", "")
	sub.theme_type_variation = &"Subtitulo"
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
		if _closed.get(section, false):
			continue
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
	if color == HELP:
		l.theme_type_variation = &"Ajuda"
	else:
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
	head.add_theme_font_size_override("font_size", 15)
	box.add_child(head)
	for k in list.size():
		var m := list[k]
		var note := _note("• " + m, color)
		note.name = "%s_%d" % [node_name, k]
		note.mouse_filter = Control.MOUSE_FILTER_STOP
		note.mouse_default_cursor_shape = Control.CURSOR_POINTING_HAND
		note.tooltip_text = "Clique para destacar na cena o que este aviso cita"
		note.gui_input.connect(func(ev: InputEvent):
			var mb := ev as InputEventMouseButton
			if mb != null and mb.pressed and mb.button_index == MOUSE_BUTTON_LEFT:
				issue_clicked.emit(String(m)))
		box.add_child(note)
	add_child(box)


## A section title that opens and closes the section (▾ open, ▸ closed).
func _section(text: String) -> void:
	add_child(HSeparator.new())
	var b := Button.new()
	b.name = "Secao_" + text.replace(" ", "_")
	b.theme_type_variation = &"TituloSecao"
	b.alignment = HORIZONTAL_ALIGNMENT_LEFT
	b.focus_mode = Control.FOCUS_NONE
	var closed: bool = _closed.get(text, false)
	b.text = ("▸  " if closed else "▾  ") + text
	b.tooltip_text = ("Abre" if closed else "Fecha") + " a seção " + text
	b.pressed.connect(func():
		_closed[text] = not _closed.get(text, false)
		(func(): show_properties(_props, _extra)).call_deferred())
	add_child(b)


func _row(field: Dictionary) -> HBoxContainer:
	var row := HBoxContainer.new()
	var label := Label.new()
	label.text = field["label"]
	# The label wraps instead of being cut (G2 visual check): long names like "Tempo entre
	# chegadas" take two lines, and the tooltip has the whole text with the help.
	label.custom_minimum_size = Vector2(LABEL_WIDTH - 14 * int(field["depth"]), 0)
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.size_flags_vertical = Control.SIZE_SHRINK_CENTER
	label.tooltip_text = String(field["label"]) + ("
" + String(field["help"]) if String(field["help"]) != "" else "")
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
			# Long text wraps inside the panel instead of scrolling sideways.
			t.wrap_mode = TextEdit.LINE_WRAPPING_BOUNDARY
			t.scroll_fit_content_height = true
			add_child(t)
			var apply := Button.new()
			apply.name = widget_name(path) + "_aplicar"
			apply.text = "Aplicar" if path == "description" else "Aplicar script"
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
			o.fit_to_longest_item = false
			o.name = widget_name(path)
			o.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			o.clip_text = true
			o.text_overrun_behavior = TextServer.OVERRUN_TRIM_ELLIPSIS
			o.tooltip_text = field["help"]
			var values := []
			for c in field["choices"]:
				o.add_item(c["label"])
				values.append(c["value"])
			o.selected = values.find(field["value"])
			# Shrinks with the panel (the text is clipped, the label next to it is not); the
			# tooltip has the whole choice.
			o.custom_minimum_size = Vector2(60, 0)
			if o.selected >= 0:
				o.tooltip_text = o.get_item_text(o.selected) + ("
" + String(field["help"]) if String(field["help"]) != "" else "")
			o.item_selected.connect(func(k): edited.emit(path, values[k]))
			row.add_child(o)
		"ron":
			var row := _row(field)
			if field["optional"]:
				_toggle(row, field)
			if field["value"] == null:
				return
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
				box.add_child(_note("Nenhum transportador no modelo.", HELP))
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


## A text field committed on Enter or when it loses focus, only when the text differs
## from the last one sent (Enter and then losing focus is one edit, not two undo steps).
func _line(path: String, value: String) -> LineEdit:
	var e := LineEdit.new()
	e.name = widget_name(path)
	e.text = value
	e.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	e.select_all_on_focus = true
	# Lambdas capture by value: the last text sent goes in a dictionary.
	var sent := {"text": value}
	var commit := func(_t = null):
		if e.text != sent["text"]:
			sent["text"] = e.text
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
	var data: Dictionary = (_props.get("data", {}) as Dictionary).get(path, {})
	if not data.is_empty():
		var applied: bool = data.get("source", "") == "dados"
		var key := draft_key(path)
		if applied:
			var seal := Label.new()
			seal.name = widget_name(path) + "_selo"
			seal.text = "dados"
			seal.theme_type_variation = &"Selo"
			seal.size_flags_vertical = Control.SIZE_SHRINK_CENTER
			seal.tooltip_text = "Este campo vem de dados reais: " + String(data.get("label", ""))
			row.add_child(seal)
		var indent_src := 14 * (int(field["depth"]) + 1)
		var using_data: bool = applied or _data_mode.get(key, false)
		var src_row := HBoxContainer.new()
		var src_pad := Control.new()
		src_pad.custom_minimum_size = Vector2(indent_src, 0)
		src_row.add_child(src_pad)
		var src_label := Label.new()
		src_label.text = "Fonte"
		src_label.theme_type_variation = &"RotuloCampo"
		src_label.custom_minimum_size = Vector2(LABEL_WIDTH - indent_src, 0)
		src_row.add_child(src_label)
		var src := OptionButton.new()
		src.name = widget_name(path) + "_fonte"
		src.fit_to_longest_item = false
		src.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		src.custom_minimum_size = Vector2(60, 0)
		src.add_item("Distribuição")
		src.add_item("Dados reais")
		src.selected = 1 if using_data else 0
		src.tooltip_text = "De onde vem a distribuição deste campo: parâmetros informados ou valores reais, ajustados automaticamente"
		src.item_selected.connect(func(k):
			if k == 1:
				_data_mode[key] = true
				redraw()
			elif applied:
				_data_mode.erase(key)
				data_to_dist.emit(path)
			else:
				_data_mode.erase(key)
				redraw())
		src_row.add_child(src)
		add_child(src_row)
		if using_data:
			var box := DadosReais.new()
			box.name = widget_name(path) + "_dados"
			add_child(box)
			box.build(self, path, data, indent_src)
			return
	var family := OptionButton.new()
	family.fit_to_longest_item = false
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
			l.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
			l.tooltip_text = String(p["label"])
			l.mouse_filter = Control.MOUSE_FILTER_PASS
			h.add_child(l)
			var fake := {"type": "int" if p["integer"] else "float", "value": p["value"], "min": null, "unit": "", "help": ""}
			h.add_child(_spin(fake, path + "." + String(p["key"])))
			box.add_child(h)
	var mean := Label.new()
	mean.text = "média %s" % _num(float(view["mean"]))
	mean.theme_type_variation = &"Ajuda"
	var mh := HBoxContainer.new()
	var mpad := Control.new()
	mpad.custom_minimum_size = Vector2(indent, 0)
	mh.add_child(mpad)
	mh.add_child(mean)
	box.add_child(mh)


## Key of the draft of the data form of `path` (per selected object).
func draft_key(path: String) -> String:
	return "%s|%s" % [_props.get("name", ""), path]


## Draws the panel again with the same properties (after a local choice).
func redraw() -> void:
	(func(): show_properties(_props, _extra)).call_deferred()


## Asks for the analysis of an applied entry that has none yet (once per entry).
func request_analysis(path: String, draft: Dictionary) -> void:
	var k := draft_key(path)
	if _requested.has(k):
		return
	_requested[k] = true
	data_analyze.emit.call_deferred(path, draft)


func clear_requests() -> void:
	_requested.clear()


## The file chosen in the dialog for the data of `path`.
func set_data_file(path: String, file: String) -> void:
	var k := draft_key(path)
	if drafts.has(k):
		drafts[k]["source"] = file
		drafts[k]["column"] = ""
		drafts[k]["origem"] = "arquivo"
	redraw()


## The asset of the look: a search box and the library and catalogue entries, with the
## credits as tooltips.
func _asset(field: Dictionary) -> void:
	var path: String = field["path"]
	var row := _row(field)
	var current: String = field["value"]
	var o := OptionButton.new()
	o.fit_to_longest_item = false
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
	# The miniature follows the entry under the mouse while the list is open.
	var popup := o.get_popup()
	popup.id_focused.connect(func(id):
		var k := popup.get_item_index(id)
		if k >= 0 and k < values.size() and values[k] != null:
			_show_miniature(values[k]))
	popup.popup_hide.connect(func(): _show_miniature(current))
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
		add_child(_note(field["help"], HELP))
	_miniature(path)
	_show_miniature(current)


## The miniature's small 3D world: a camera, a light and the object's scene.
func _miniature(path: String) -> void:
	_mini_asset = "-"
	if not look_provider.is_valid():
		return
	var box := SubViewportContainer.new()
	box.name = widget_name(path) + "_miniatura"
	box.stretch = true
	box.custom_minimum_size = Vector2(0, 150)
	box.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	box.tooltip_text = "Miniatura do objeto com o ativo escolhido (ou o que está sob o mouse na lista)"
	_mini_view = SubViewport.new()
	_mini_view.own_world_3d = true
	_mini_view.msaa_3d = Viewport.MSAA_2X
	box.add_child(_mini_view)
	var env := WorldEnvironment.new()
	var e := Environment.new()
	e.background_mode = Environment.BG_COLOR
	e.background_color = Color(0.16, 0.17, 0.21)
	e.ambient_light_source = Environment.AMBIENT_SOURCE_COLOR
	e.ambient_light_color = Color(0.80, 0.84, 0.92)
	e.ambient_light_energy = 0.8
	e.tonemap_mode = Environment.TONE_MAPPER_AGX
	env.environment = e
	_mini_view.add_child(env)
	var sun := DirectionalLight3D.new()
	sun.light_energy = 1.4
	sun.transform = Transform3D(Basis.IDENTITY, Vector3(4, 8, 5)).looking_at(Vector3.ZERO, Vector3.UP)
	_mini_view.add_child(sun)
	_mini_camera = Camera3D.new()
	_mini_camera.current = true
	_mini_view.add_child(_mini_camera)
	_mini_scene = CenaUtsim.new()
	_mini_scene.show_floor = false
	_mini_scene.show_connections = false
	_mini_view.add_child(_mini_scene)
	add_child(box)


## Draws the object with `asset` in the miniature (only when it changed).
func _show_miniature(asset: String) -> void:
	if _mini_scene == null or not is_instance_valid(_mini_scene) or asset == _mini_asset:
		return
	_mini_asset = asset
	var pb: Object = look_provider.call(asset)
	if pb == null:
		_mini_scene.clear()
		return
	_mini_scene.build(pb)
	_mini_scene.apply(0.0, 0.0, true)
	_mini_scene.set_labels_visible(false)
	# Frame the object from the front-right, a little above.
	var lo: Vector3 = _mini_scene.info.get("bounds_min", Vector3(-1, 0, -1))
	var hi: Vector3 = _mini_scene.info.get("bounds_max", Vector3(1, 1, 1))
	var size := maxf((hi - lo).length(), 1.0)
	var center := (lo + hi) * 0.5
	var eye := center + Vector3(0.9, 0.7, 1.1).normalized() * size * 1.1
	_mini_camera.transform = Transform3D(Basis.IDENTITY, eye).looking_at(center, Vector3.UP)


## Outputs in port order, with ▲ ▼ to reorder and ✕ to remove; then the inputs.
func _outputs(props: Dictionary) -> void:
	var outs: PackedStringArray = props.get("outputs", PackedStringArray())
	var ins: PackedStringArray = props.get("inputs", PackedStringArray())
	var head := Label.new()
	head.text = "Ligações de saída (ordem das portas)"
	head.add_theme_color_override("font_color", Color(0.85, 0.88, 0.94))
	add_child(head)
	if outs.is_empty():
		add_child(_note("Nenhuma. Use a ferramenta Ligar (L).", HELP))
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
		add_child(_note("Entradas: " + ", ".join(ins), HELP))


static func _num(x: float) -> String:
	if x == 0.0 or is_nan(x) or is_inf(x):
		return str(x)
	var digits := clampi(3 - int(floor(log(absf(x)) / log(10.0))), 0, 6)
	return String.num(x, digits)
