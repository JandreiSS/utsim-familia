## The object library of Editar ("Biblioteca"): the entries of `utsim_edit::PALETTE`
## grouped by `CATEGORIES`, with the icons of `res://icones/<id>.svg`.
##
## Clicking an entry arms it (`chosen`); pressing on it and dragging out of the panel
## starts a drag (`drag_started`), which the editor follows until the button is released
## over the floor. Each button is named `Biblioteca_<id>` so scripts can find it.
class_name Biblioteca
extends VBoxContainer

signal chosen(id: String)
signal drag_started(id: String)

var _buttons := {}
var _pressed_id := ""
var _press_at := Vector2.ZERO


func build(palette: Array, categories: PackedStringArray) -> void:
	for c in get_children():
		c.queue_free()
	_buttons.clear()
	var title := Label.new()
	title.text = "Biblioteca"
	title.add_theme_font_size_override("font_size", 16)
	add_child(title)
	for cat in categories:
		var head := Label.new()
		head.text = cat
		head.add_theme_color_override("font_color", Color(0.65, 0.70, 0.78))
		head.add_theme_font_size_override("font_size", 12)
		add_child(head)
		var grid := GridContainer.new()
		grid.columns = 2
		grid.add_theme_constant_override("h_separation", 4)
		grid.add_theme_constant_override("v_separation", 4)
		add_child(grid)
		for e in palette:
			if e["category"] != cat:
				continue
			var id: String = e["id"]
			var b := Button.new()
			b.name = "Biblioteca_" + id
			b.text = e["label"]
			b.alignment = HORIZONTAL_ALIGNMENT_LEFT
			b.toggle_mode = true
			b.custom_minimum_size = Vector2(124, 34)
			b.size_flags_horizontal = Control.SIZE_EXPAND_FILL
			b.tooltip_text = "%s: clique e depois clique no chão, ou arraste até o chão" % e["label"]
			b.add_theme_constant_override("icon_max_width", 22)
			var icon_path := "res://icones/%s.svg" % id
			if ResourceLoader.exists(icon_path):
				b.icon = load(icon_path)
			b.gui_input.connect(_on_input.bind(id))
			grid.add_child(b)
			_buttons[id] = b


## Shows entry `id` as armed (or none for `""`).
func mark(id: String) -> void:
	for k in _buttons:
		(_buttons[k] as Button).set_pressed_no_signal(k == id)


func _on_input(event: InputEvent, id: String) -> void:
	var mb := event as InputEventMouseButton
	if mb != null and mb.button_index == MOUSE_BUTTON_LEFT:
		if mb.pressed:
			_pressed_id = id
			_press_at = mb.global_position
		elif _pressed_id == id:
			_pressed_id = ""
			chosen.emit(id)
		accept_event()
		return
	var mm := event as InputEventMouseMotion
	if mm != null and _pressed_id == id and (mm.button_mask & MOUSE_BUTTON_MASK_LEFT) != 0:
		if mm.global_position.distance_to(_press_at) > 8.0:
			_pressed_id = ""
			drag_started.emit(id)
