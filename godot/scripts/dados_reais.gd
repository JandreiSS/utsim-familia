## The "Dados reais" half of a distribution field in the properties panel: where the
## values come from (CSV file and column, or pasted values; instants or durations), how to
## choose (best automatic, empirical or a fixed family) and by which criterion, and, once
## applied, the result: family, parameters, n, a small histogram with the curve, the
## warnings about the data and a short ranking of the candidates to swap with one click.
##
## Like the rest of the panel it never changes the model: it keeps a draft of the choices
## (in the panel, so it survives redraws) and emits `data_apply`, `data_analyze`,
## `data_to_dist`, `data_report` and `data_pick_file` through the panel. The scene applies
## them (the fit runs on the bridge's thread; undo and redo go through `SetFieldSource`).
##
## Widgets are named `Prop_<path>_<part>` (dots as underscores): `origem`, `arquivo`,
## `escolher`, `coluna`, `valores`, `instantes`, `escolha`, `criterio`, `aplicar`,
## `resultado`, `parametros`, `grafico`, `reajustar`, `cand_<k>`, `cand_empirica` and
## `relatorio`.
class_name DadosReais
extends VBoxContainer

const CRITERIA := [["aic", "AIC"], ["bic", "BIC"], ["ks", "Kolmogorov-Smirnov"], ["ad", "Anderson-Darling"]]

var panel: PainelPropriedades
var path := ""
var key := ""
var entry := {}
var draft := {}
var indent := 14


## The draft (the choices of the form) of an applied entry.
static func draft_of(config: Dictionary) -> Dictionary:
	var values: Array = config.get("values", [])
	var lines := PackedStringArray()
	for v in values:
		lines.append(str(v))
	return {
		"origem": "valores" if not values.is_empty() else "arquivo",
		"source": config.get("source", ""),
		"column": config.get("column", ""),
		"text": "\n".join(lines),
		"timestamps": config.get("timestamps", false),
		"choose": config.get("choose", "Best"),
		"criterion": config.get("criterion", "aic"),
	}


func build(p: PainelPropriedades, field_path: String, data: Dictionary, pad: int) -> void:
	panel = p
	path = field_path
	entry = data
	indent = pad
	key = p.draft_key(path)
	var applied: bool = entry.get("source", "") == "dados"
	if applied:
		var config: Dictionary = entry.get("config", {})
		# A different applied entry (undo, redo, another fit): the form follows it.
		if p.drafts_seen.get(key) != config:
			p.drafts_seen[key] = config
			p.drafts[key] = draft_of(config)
	elif not p.drafts.has(key):
		p.drafts[key] = {
			"origem": "arquivo", "source": "", "column": "", "text": "",
			"timestamps": false, "choose": "Best", "criterion": "aic",
		}
	draft = p.drafts[key]
	_form()
	if applied:
		_result()


func _name(part: String) -> String:
	return PainelPropriedades.widget_name(path) + "_" + part


## A form row: indentation, label and the control (which takes the rest).
func _line(label: String, control: Control) -> HBoxContainer:
	var h := HBoxContainer.new()
	var pad := Control.new()
	pad.custom_minimum_size = Vector2(indent, 0)
	h.add_child(pad)
	var l := Label.new()
	l.text = label
	l.theme_type_variation = &"RotuloCampo"
	l.custom_minimum_size = Vector2(PainelPropriedades.LABEL_WIDTH - indent, 0)
	l.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	h.add_child(l)
	control.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	h.add_child(control)
	add_child(h)
	return h


func _option(part: String, items: Array, selected: int, on_pick: Callable) -> OptionButton:
	var o := OptionButton.new()
	o.name = _name(part)
	o.fit_to_longest_item = false
	o.clip_text = true
	o.custom_minimum_size = Vector2(60, 0)
	for it in items:
		o.add_item(it)
	o.selected = selected
	o.item_selected.connect(on_pick)
	return o


func _note(text: String, variation: StringName) -> Label:
	var l := Label.new()
	l.text = text
	l.theme_type_variation = variation
	l.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	l.custom_minimum_size = Vector2(240, 0)
	return l


func _indented(c: Control) -> HBoxContainer:
	var h := HBoxContainer.new()
	var pad := Control.new()
	pad.custom_minimum_size = Vector2(indent, 0)
	h.add_child(pad)
	c.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	h.add_child(c)
	add_child(h)
	return h


func _form() -> void:
	var from_file: bool = draft["origem"] == "arquivo"
	_line("Origem", _option("origem", ["Arquivo CSV", "Valores colados"], 0 if from_file else 1, func(k):
		draft["origem"] = "arquivo" if k == 0 else "valores"
		panel.redraw()))
	if from_file:
		var row := HBoxContainer.new()
		var file := LineEdit.new()
		file.name = _name("arquivo")
		file.placeholder_text = "arquivo .csv ou .txt"
		file.text = draft["source"]
		file.size_flags_horizontal = Control.SIZE_EXPAND_FILL
		file.text_changed.connect(func(t): draft["source"] = t)
		# The columns of a typed path are listed when the field is committed.
		file.text_submitted.connect(func(_t): panel.redraw())
		row.add_child(file)
		var pick := Button.new()
		pick.name = _name("escolher")
		pick.text = "Escolher..."
		pick.tooltip_text = "Escolhe o arquivo de dados (CSV ou TXT)"
		pick.pressed.connect(func(): panel.data_pick_file.emit(path))
		row.add_child(pick)
		_line("Arquivo", row)
		var columns: Array = []
		if String(draft["source"]).strip_edges() != "" and panel.columns_provider.is_valid():
			columns = panel.columns_provider.call(String(draft["source"]).strip_edges())
		var items := ["(automática)"]
		items.append_array(columns)
		var at := maxi(items.find(draft["column"]), 0)
		var col := _option("coluna", items, at, func(k):
			draft["column"] = "" if k == 0 else items[k])
		col.disabled = columns.is_empty()
		col.tooltip_text = "Coluna do arquivo com os valores"
		_line("Coluna", col)
	else:
		var t := TextEdit.new()
		t.name = _name("valores")
		t.placeholder_text = "um valor por linha (ou separados por ; ou espaço)"
		t.text = draft["text"]
		t.custom_minimum_size = Vector2(0, 70)
		t.scroll_fit_content_height = true
		t.wrap_mode = TextEdit.LINE_WRAPPING_BOUNDARY
		t.text_changed.connect(func(): draft["text"] = t.text)
		_indented(t)
	var inst := CheckBox.new()
	inst.name = _name("instantes")
	inst.text = "São instantes (usar os intervalos entre eles)"
	inst.button_pressed = draft["timestamps"]
	inst.tooltip_text = "Marque quando os valores são instantes crescentes (como horas de chegada): o ajuste usa as diferenças entre eles. Desmarcado, os valores são durações."
	inst.toggled.connect(func(on): draft["timestamps"] = on)
	_indented(inst)
	var choices := ["Best", "Empirical"]
	var labels := ["Melhor automática", "Empírica (os próprios valores)"]
	for f in panel.data_families:
		choices.append(f)
		labels.append("Só " + f)
	_line("Escolha", _option("escolha", labels, maxi(choices.find(draft["choose"]), 0), func(k):
		draft["choose"] = choices[k]))
	var keys := []
	var names := []
	for c in CRITERIA:
		keys.append(c[0])
		names.append(c[1])
	var crit := _option("criterio", names, maxi(keys.find(draft["criterion"]), 0), func(k):
		draft["criterion"] = keys[k])
	crit.tooltip_text = "O que ordena as famílias aceitas pelos testes"
	_line("Critério", crit)
	var apply := Button.new()
	apply.name = _name("aplicar")
	apply.text = "Aplicar" if entry.get("source", "") != "dados" else "Reajustar com estas opções"
	apply.tooltip_text = "Ajusta a distribuição aos dados (em segundo plano) e passa a usá-la neste campo"
	apply.pressed.connect(func(): panel.data_apply.emit(path, draft.duplicate(true)))
	_indented(apply)


func _result() -> void:
	add_child(HSeparator.new())
	var err := String(entry.get("error", ""))
	if err != "":
		_indented(_note("Não foi possível ler os dados: " + err, &"ErroDados"))
	if bool(entry.get("stale", false)):
		_indented(_note("Os dados mudaram desde o último ajuste.", &"AvisoDados"))
		var redo := Button.new()
		redo.name = _name("reajustar")
		redo.text = "Reajustar aos dados atuais"
		redo.pressed.connect(func(): panel.data_apply.emit(path, draft.duplicate(true)))
		_indented(redo)
	elif String(entry.get("changed_on_load", "")) != "":
		_indented(_note(String(entry["changed_on_load"]) + ". O campo foi reajustado; salve para registrar.", &"AvisoDados"))
	var view: Variant = entry.get("view")
	var family := String(entry.get("family", ""))
	var res := Label.new()
	res.name = _name("resultado")
	res.text = "%s · n = %s · %s" % [family, str(entry.get("n", "?")), String(entry.get("label", ""))]
	res.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	res.custom_minimum_size = Vector2(240, 0)
	_indented(res)
	if typeof(view) != TYPE_DICTIONARY:
		if err == "":
			_indented(_note("Analisando os dados...", &"Ajuda"))
			panel.request_analysis(path, draft.duplicate(true))
		return
	var v: Dictionary = view
	var parts := PackedStringArray()
	for p in v.get("params", []):
		parts.append("%s = %s" % [p["label"], PainelPropriedades._num(float(p["value"]))])
	if not parts.is_empty():
		var pl := _note("   ".join(parts), &"Ajuda")
		pl.name = _name("parametros")
		_indented(pl)
	var chart := MiniHistograma.new()
	chart.name = _name("grafico")
	chart.set_data(v.get("chart", {}))
	chart.tooltip_text = "Histograma dos dados e a curva da distribuição escolhida"
	_indented(chart)
	for w in v.get("warnings", []):
		_indented(_note("• " + String(w), &"AvisoDados"))
	var rank: Array = v.get("ranking", [])
	if not rank.is_empty():
		var head := Label.new()
		head.text = "Candidatas (%s)" % String(v.get("criterion", ""))
		head.theme_type_variation = &"RotuloSecao"
		_indented(head)
	for k in rank.size():
		var r: Dictionary = rank[k]
		var b := Button.new()
		b.name = _name("cand_%d" % k)
		b.alignment = HORIZONTAL_ALIGNMENT_LEFT
		b.clip_text = true
		b.text = "%d. %s   %s   %s" % [k + 1, r["family"], PainelPropriedades._num(float(r["criterion"])), r["status"]]
		b.tooltip_text = "%s\nAIC %s   BIC %s\nUsa esta família neste campo" % [
			r["params"], PainelPropriedades._num(float(r["aic"])), PainelPropriedades._num(float(r["bic"]))]
		b.disabled = bool(r["chosen"])
		var fam: String = r["family"]
		b.pressed.connect(func():
			var d := draft.duplicate(true)
			d["choose"] = fam
			panel.data_apply.emit(path, d))
		_indented(b)
	if not bool(v.get("empirical", false)):
		var e := Button.new()
		e.name = _name("cand_empirica")
		e.alignment = HORIZONTAL_ALIGNMENT_LEFT
		e.text = "Usar a empírica (os próprios valores)"
		e.pressed.connect(func():
			var d := draft.duplicate(true)
			d["choose"] = "Empirical"
			panel.data_apply.emit(path, d))
		_indented(e)
	var rep := Button.new()
	rep.name = _name("relatorio")
	rep.text = "Relatório"
	rep.tooltip_text = "Grava o relatório completo do ajuste (HTML) numa pasta e abre no navegador"
	rep.pressed.connect(func(): panel.data_report.emit())
	_indented(rep)
