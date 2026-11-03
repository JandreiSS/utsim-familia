## The "Resultados" panel (phase G3), over the 3D view: the numbers of the last run
## (`UtsimResults`, the same as `utsim run` and the Bevy results window) in five tabs.
##
## - **Resumo:** run settings, the bottleneck overall and within shift, the table per
##   object with the 95% intervals and the CSV export.
## - **Gráficos:** utilisation (with whiskers), states (the colours of the 3D view), the
##   content over time and the pooled histograms, drawn by `Grafico`.
## - **Análise:** how much to trust the numbers (`docs/plano_analises_e_dinamica.md`,
##   section 2): warm-up (Welch's curve and the MSER-5 cut, with "Aplicar" writing
##   `run.warmup` to the model), replications needed (with "Rodar mais"), Little's law with
##   a traffic light per object, and the shifting bottleneck. Each one says in one line
##   what it means.
## - **Cenários:** scenarios written as text, compared with common random numbers.
## - **Otimização:** one objective, integer or continuous variables, a method.
##
## Every run goes through `main.start_job` (a thread of the bridge, with progress and
## "Cancelar"), so the interface keeps drawing.
class_name PainelResultados
extends PanelContainer

const CHARTS := ["Ocupação", "Estados", "Série temporal", "Histograma"]

var main: Node
## The `UtsimResults` shown.
var results: Object
var _tabs: TabContainer
var _summary: RichTextLabel
var _table: RichTextLabel
var _export_dir: LineEdit
var _chart_kind: OptionButton
var _chart_object: OptionButton
var _chart_extra: OptionButton
var _chart: Grafico
var _chart_note: Label
var _warm_series: LineEdit
var _warm_window: SpinBox
var _warm_chart: Grafico
var _warm_text: Label
var _warm_apply: Button
var _warm_suggested := NAN
var _prec_target: LineEdit
var _prec_text: RichTextLabel
var _prec_more: Button
var _prec_more_n := 0
var _little_text: RichTextLabel
var _shift_chart: Grafico
var _shift_text: Label
var _sc_text: TextEdit
var _sc_measures: LineEdit
var _sc_reps: SpinBox
var _sc_result: RichTextLabel
var _op_metric: LineEdit
var _op_max: CheckBox
var _op_vars: TextEdit
var _op_method: OptionButton
var _op_reps: SpinBox
var _op_budget: SpinBox
var _op_result: RichTextLabel
var _op_chart: Grafico
var _objects: Array = []


func build(owner_main: Node) -> void:
	main = owner_main
	name = "PainelResultados"
	var root := VBoxContainer.new()
	add_child(root)
	var head := HBoxContainer.new()
	root.add_child(head)
	var title := Label.new()
	title.text = "Resultados"
	title.add_theme_font_size_override("font_size", 18)
	head.add_child(title)
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(spacer)
	_button(head, "Res_Fechar", "Fechar", "Fechar os resultados (a cena continua)", func(): visible = false)
	_tabs = TabContainer.new()
	_tabs.name = "Res_Abas"
	_tabs.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add_child(_tabs)
	_build_summary()
	_build_charts()
	_build_analysis()
	_build_scenarios()
	_build_optimize()


# ---------------------------------------------------------------------------
# Building blocks
# ---------------------------------------------------------------------------


func _button(parent: Node, id: String, text: String, tip: String, action: Callable) -> Button:
	var b := Button.new()
	b.name = id
	b.text = text
	b.tooltip_text = tip
	b.focus_mode = Control.FOCUS_NONE
	b.pressed.connect(action)
	parent.add_child(b)
	return b


func _tab(title: String) -> VBoxContainer:
	var scroll := ScrollContainer.new()
	scroll.name = "Res_Aba_" + title
	scroll.horizontal_scroll_mode = ScrollContainer.SCROLL_MODE_DISABLED
	_tabs.add_child(scroll)
	_tabs.set_tab_title(_tabs.get_tab_count() - 1, title)
	var v := VBoxContainer.new()
	v.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	v.add_theme_constant_override("separation", 6)
	scroll.add_child(v)
	return v


func _rich(id: String, height := 0.0) -> RichTextLabel:
	var r := RichTextLabel.new()
	r.name = id
	r.bbcode_enabled = true
	r.fit_content = height <= 0.0
	r.scroll_active = height > 0.0
	r.selection_enabled = true
	r.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	if height > 0.0:
		r.custom_minimum_size = Vector2(0, height)
	return r


func _heading(parent: Node, text: String) -> void:
	var l := Label.new()
	l.text = text
	l.add_theme_font_size_override("font_size", 16)
	l.add_theme_color_override("font_color", Color(0.62, 0.76, 0.98))
	parent.add_child(l)


func _explain(parent: Node, text: String) -> Label:
	var l := Label.new()
	l.text = text
	l.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	l.add_theme_color_override("font_color", Color(0.70, 0.74, 0.80))
	l.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	parent.add_child(l)
	return l


func _row(parent: Node) -> HBoxContainer:
	var h := HBoxContainer.new()
	h.add_theme_constant_override("separation", 6)
	parent.add_child(h)
	return h


func _label(parent: Node, text: String) -> void:
	var l := Label.new()
	l.text = text
	parent.add_child(l)


func _line(parent: Node, id: String, text: String, width := 120.0) -> LineEdit:
	var e := LineEdit.new()
	e.name = id
	e.text = text
	e.custom_minimum_size = Vector2(width, 0)
	parent.add_child(e)
	return e


func _spin(parent: Node, id: String, lo: float, hi: float, value: float) -> SpinBox:
	var s := SpinBox.new()
	s.name = id
	s.min_value = lo
	s.max_value = hi
	s.value = value
	s.focus_mode = Control.FOCUS_CLICK
	parent.add_child(s)
	return s


func _option(parent: Node, id: String, items: Array) -> OptionButton:
	var o := OptionButton.new()
	o.name = id
	o.focus_mode = Control.FOCUS_NONE
	for k in items.size():
		o.add_item(String(items[k]), k)
	parent.add_child(o)
	return o


func _chart_box(parent: Node, id: String, height := 240.0) -> Grafico:
	var g := Grafico.new()
	g.name = id
	g.custom_minimum_size = Vector2(0, height)
	g.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	parent.add_child(g)
	return g


## `mean ± half_width` with about four significant digits.
static func mh(s: Variant, percent := false) -> String:
	if s == null or not (s is Dictionary):
		return "-"
	var m = s.get("mean")
	if m == null:
		return "-"
	var k := 100.0 if percent else 1.0
	var unit := "%" if percent else ""
	var h = s.get("half_width")
	if h == null:
		return "%s%s" % [num(float(m) * k), unit]
	return "%s ± %s%s" % [num(float(m) * k), num(float(h) * k), unit]


static func num(x: float) -> String:
	if is_nan(x) or is_inf(x):
		return "-"
	if x == 0.0:
		return "0"
	var digits := clampi(3 - int(floor(log(absf(x)) / log(10.0))), 0, 6)
	return String.num(x, digits)


func _start(job: Object, done: Callable) -> void:
	if job == null:
		main.set_status("Não começou: %s" % (results.last_error() if results != null else ""))
		return
	main.start_job(job, done)


# ---------------------------------------------------------------------------
# Resumo
# ---------------------------------------------------------------------------


func _build_summary() -> void:
	var v := _tab("Resumo")
	_summary = _rich("Res_Resumo")
	v.add_child(_summary)
	_heading(v, "Por objeto (média ± semiamplitude do IC de 95 % entre replicações)")
	_table = _rich("Res_Tabela")
	v.add_child(_table)
	var h := _row(v)
	_label(h, "Exportar CSV para:")
	_export_dir = _line(h, "Res_PastaCSV", "", 320)
	_export_dir.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_button(h, "Res_ExportarCSV", "Exportar", "Grava resumo, replicações, séries e histogramas em CSV", func(): _export())


func _export() -> void:
	if results == null:
		return
	var dir := _export_dir.text.strip_edges()
	if dir == "":
		main.set_status("Escreva a pasta da exportação.")
		return
	var files: PackedStringArray = results.export_csv(dir)
	if files.is_empty():
		main.set_status("Não exportou: %s" % results.last_error())
	else:
		main.set_status("%d arquivo(s) CSV em %s." % [files.size(), dir])


## Shows `res` (a `UtsimResults`) in every tab.
func show_results(res: Object) -> void:
	results = res
	var s: Dictionary = res.summary()
	var lines := PackedStringArray()
	lines.append("[b]%s[/b]: %d replicação(ões), tempo de parada %s, aquecimento %s, semente %s." % [
		s.get("model", ""), s["replications"], num(float(s["stop_time"])), num(float(s["warmup"])), str(s["seed"])])
	var b = s.get("bottleneck")
	var bs = s.get("bottleneck_in_shift")
	lines.append("[color=#ff9a3d]Gargalo (geral): %s[/color]" % (b if b != null else "nenhum recurso ocupado"))
	lines.append("[color=#ff9a3d]Gargalo (dentro do turno): %s[/color]" % (bs if bs != null else "-"))
	lines.append("[color=#aab2be]O gargalo é o recurso mais ocupado; dentro do turno conta só o tempo em que ele trabalha (um posto parado meio dia parece ocioso no geral, mas pode estar saturado no turno).[/color]")
	_summary.text = "\n".join(lines)
	var t := PackedStringArray()
	t.append("[table=7]")
	for head in ["Objeto", "Tipo", "Entradas", "Saídas", "Conteúdo médio", "Permanência", "Ocupação"]:
		t.append("[cell][b]%s[/b]  [/cell]" % head)
	_objects = []
	for r in res.table():
		_objects.append(r["name"])
		var name: String = r["name"]
		if r["bottleneck"]:
			name = "[color=#ff9a3d]%s (gargalo)[/color]" % name
		elif r["bottleneck_in_shift"]:
			name = "[color=#ff9a3d]%s (gargalo no turno)[/color]" % name
		t.append("[cell]%s  [/cell]" % name)
		t.append("[cell]%s  [/cell]" % r["kind_label"])
		for k in ["input", "output", "content", "staytime"]:
			t.append("[cell]%s  [/cell]" % mh(r[k]))
		t.append("[cell]%s[/cell]" % (mh(r["utilization"], true) if r["resource"] else "-"))
	t.append("[/table]")
	_table.text = "".join(t)
	if _export_dir.text == "":
		var base: String = main.model_path.get_basename() if main.model_path != "" else OS.get_user_data_dir().path_join("resultados")
		_export_dir.text = base + "_resultados"
	_fill_objects()
	_update_chart()
	_prec_text.text = ""
	_prec_more.disabled = true


# ---------------------------------------------------------------------------
# Gráficos
# ---------------------------------------------------------------------------


func _build_charts() -> void:
	var v := _tab("Gráficos")
	var h := _row(v)
	_label(h, "Gráfico:")
	_chart_kind = _option(h, "Res_TipoGrafico", CHARTS)
	_chart_kind.item_selected.connect(func(_k): _update_chart())
	_label(h, "Objeto:")
	_chart_object = _option(h, "Res_ObjetoGrafico", [])
	_chart_object.item_selected.connect(func(_k): _update_chart(true))
	_chart_extra = _option(h, "Res_DetalheGrafico", ["trabalho", "turno", "bloqueio"])
	_chart_extra.item_selected.connect(func(_k): _update_chart())
	_chart_note = _explain(v, "")
	_chart = _chart_box(v, "Res_Grafico", 320)


func _fill_objects() -> void:
	var keep := _chart_object.get_item_text(_chart_object.selected) if _chart_object.selected >= 0 else ""
	_chart_object.clear()
	for k in _objects.size():
		_chart_object.add_item(_objects[k], k)
	var want: String = keep
	if want == "" and results != null:
		var b = results.summary().get("bottleneck")
		want = String(b) if b != null else ""
	for k in _objects.size():
		if _objects[k] == want:
			_chart_object.select(k)


## Chart kind by name (`ocupacao`, `estados`, `serie`, `histograma`): scripts.
func choose_chart(which: String) -> void:
	var k := ["ocupacao", "estados", "serie", "histograma"].find(which)
	if k >= 0:
		_chart_kind.select(k)
		_update_chart()


func _update_chart(object_changed := false) -> void:
	if results == null:
		return
	var kind := _chart_kind.selected
	var object := _chart_object.get_item_text(_chart_object.selected) if _chart_object.selected >= 0 else ""
	_chart_object.visible = kind >= 2
	if kind == 3 and (object_changed or _chart_extra.item_count == 0 or _chart_extra.get_item_text(0) == "trabalho"):
		_chart_extra.clear()
		var tallies: PackedStringArray = results.tallies(object)
		for k in tallies.size():
			_chart_extra.add_item(tallies[k], k)
	elif kind == 0 and (_chart_extra.item_count == 0 or _chart_extra.get_item_text(0) != "trabalho"):
		_chart_extra.clear()
		for k in 3:
			_chart_extra.add_item(["trabalho", "turno", "bloqueio"][k], k)
	_chart_extra.visible = kind == 0 or kind == 3
	match kind:
		0:
			var rows := []
			for r in results.utilization(_chart_extra.get_item_text(maxi(_chart_extra.selected, 0))):
				rows.append({"label": r["name"], "value": _pct(r["mean"]), "half_width": _pct(r["half_width"]), "hot": r["bottleneck"]})
			_chart.bars(rows, "%", 100.0)
			_chart_note.text = "Fração do tempo trabalhando (turno: só o tempo dentro do turno; bloqueio: somando o tempo bloqueado). A barra laranja é o gargalo; o traço é o IC de 95 %."
		1:
			var rows := []
			for r in results.states():
				rows.append({"label": r["name"], "segments": r["segments"]})
			_chart.stacked(rows)
			_chart_note.text = "Fração do tempo em cada estado, com as cores da cena 3D."
		2:
			var s: Dictionary = results.series(object)
			var series := []
			for curve in s.get("replications", []):
				series.append({"points": curve, "color": Color(0.35, 0.45, 0.60, 0.6), "width": 1.0})
			if not (s.get("mean", []) as Array).is_empty():
				series.append({"points": s["mean"], "color": Color(0.98, 0.80, 0.35), "width": 2.5})
			_chart.empty_text = "sem série: ligue run.sample_interval no modelo"
			_chart.lines(series, {"x_label": "tempo", "y_label": "conteúdo de %s" % object})
			_chart_note.text = "Conteúdo de %s no tempo: cada replicação em cinza e a média em amarelo." % object
		3:
			var tally := _chart_extra.get_item_text(_chart_extra.selected) if _chart_extra.selected >= 0 else "staytime"
			_chart.empty_text = "sem histograma: ligue run.histogram_bins no modelo"
			_chart.histogram(results.histogram(object, tally))
			_chart_note.text = "Histograma de %s.%s, juntando todas as replicações." % [object, tally]


static func _pct(x: Variant) -> Variant:
	return null if x == null else float(x) * 100.0


# ---------------------------------------------------------------------------
# Análise
# ---------------------------------------------------------------------------


func _build_analysis() -> void:
	var v := _tab("Análise")
	_heading(v, "Aquecimento")
	_explain(v, "Roda as replicações desde o tempo 0, sem aquecimento, e mostra a média entre elas (curva de Welch). O corte do MSER-5 é onde a série deixa de ter o começo vazio; \"Aplicar\" grava esse tempo em run.warmup.")
	var h := _row(v)
	_label(h, "Série:")
	_warm_series = _line(h, "Res_AquecSerie", "wip", 200)
	_warm_series.tooltip_text = "wip (conteúdo somado) ou objeto.medida (saida.time_in_system)"
	_label(h, "Janela:")
	_warm_window = _spin(h, "Res_AquecJanela", 1, 200, 5)
	_button(h, "Res_AquecAnalisar", "Analisar", "Roda e calcula Welch e MSER-5", func(): _start_warmup())
	_warm_apply = _button(h, "Res_AquecAplicar", "Aplicar", "Grava o corte sugerido em run.warmup do modelo", func(): _apply_warmup())
	_warm_apply.disabled = true
	_warm_text = _explain(v, "")
	_warm_text.name = "Res_AquecTexto"
	_warm_chart = _chart_box(v, "Res_AquecGrafico", 220)

	_heading(v, "Replicações necessárias")
	_explain(v, "Quantas replicações faltam para a semiamplitude do IC de 95 % ficar abaixo do alvo (5% é relativo à média; 0.5 é absoluto). \"Rodar mais\" acrescenta as que faltam às já feitas, com sementes novas.")
	h = _row(v)
	_label(h, "Precisão:")
	_prec_target = _line(h, "Res_Precisao", "5%", 80)
	_button(h, "Res_PrecisaoCalcular", "Calcular", "Calcula as replicações necessárias", func(): _precision())
	_prec_more = _button(h, "Res_RodarMais", "Rodar mais", "Roda as replicações que faltam e junta às atuais", func(): _run_more())
	_prec_more.disabled = true
	_prec_text = _rich("Res_PrecisaoTexto")
	v.add_child(_prec_text)

	_heading(v, "Consistência: lei de Little")
	_explain(v, "Confere L = λ·W em cada objeto e no sistema. Verde: fecha; amarelo: a diferença é estatisticamente diferente de zero, mas pequena (até 2 %); vermelho: não fecha, o que costuma indicar aquecimento curto ou itens presos no fim.")
	_button(_row(v), "Res_Little", "Conferir", "Roda as replicações e confere a lei de Little", func(): _start_little())
	_little_text = _rich("Res_LittleTexto")
	v.add_child(_little_text)

	_heading(v, "Gargalo móvel")
	_explain(v, "Em cada instante, o gargalo é o recurso no período ativo mais longo; quando dois períodos se sobrepõem, o gargalo é compartilhado. Mostra a fração do tempo de cada recurso como gargalo único e compartilhado.")
	_button(_row(v), "Res_GargaloMovel", "Calcular", "Roda as replicações com o rastro e calcula o gargalo móvel", func(): _start_shifting())
	_shift_text = _explain(v, "")
	_shift_text.name = "Res_GargaloMovelTexto"
	_shift_chart = _chart_box(v, "Res_GargaloMovelGrafico", 160)


func _start_warmup() -> void:
	if results == null:
		return
	var job: Object = results.start_warmup(_warm_series.text.strip_edges(), int(_warm_window.value))
	_start(job, func(j): _show_warmup(j.data()))


func _show_warmup(d: Variant) -> void:
	if d == null:
		return
	var series := []
	series.append({"points": d["mean"], "color": Color(0.40, 0.50, 0.65, 0.7), "width": 1.0})
	series.append({"points": d["welch"], "color": Color(0.98, 0.80, 0.35), "width": 2.5})
	var cut = d.get("suggested")
	_warm_suggested = float(cut) if cut != null else NAN
	var markers := []
	if cut != null:
		markers.append({"x": float(cut), "label": "MSER-5: %s" % num(float(cut))})
	_warm_chart.lines(series, {"markers": markers, "x_label": "tempo", "y_label": d["series"]})
	if cut == null:
		_warm_text.text = "Série curta demais para o MSER-5 (%d pontos): aumente o tempo de parada ou a amostragem." % int(d["points"])
	elif d.get("too_short", false):
		_warm_text.text = "Corte sugerido: %s, mas o mínimo ficou na segunda metade da série: a rodada é curta ou não se estabiliza." % num(_warm_suggested)
	else:
		_warm_text.text = "Corte sugerido (MSER-5): %s. Antes dele a média ainda sobe ou desce." % num(_warm_suggested)
	_warm_apply.disabled = cut == null
	main.set_status("Aquecimento analisado: %s." % _warm_text.text)


func _apply_warmup() -> void:
	if is_nan(_warm_suggested):
		return
	main.apply_warmup(_warm_suggested)


func _precision() -> void:
	if results == null:
		return
	var d = results.precision(_prec_target.text.strip_edges(), PackedStringArray())
	if d == null:
		_prec_text.text = "[color=#ff7a70]%s[/color]" % results.last_error()
		_prec_more.disabled = true
		return
	var t := PackedStringArray()
	t.append("[table=4]")
	for head in ["Medida", "Atual", "Alvo (±)", "Faltam"]:
		t.append("[cell][b]%s[/b]  [/cell]" % head)
	for r in d["rows"]:
		t.append("[cell]%s  [/cell]" % r["metric"])
		t.append("[cell]%s  [/cell]" % mh(r["current"]))
		t.append("[cell]%s  [/cell]" % num(float(r["target"])) if r["target"] != null else "[cell]-[/cell]")
		var more = r.get("more")
		t.append("[cell]%s[/cell]" % (str(more) if more != null else "?"))
	t.append("[/table]")
	_prec_more_n = int(d.get("more", 0))
	if _prec_more_n > 0:
		t.append("\nPara todas as medidas chegarem ao alvo faltam [b]%d[/b] replicação(ões)." % _prec_more_n)
	else:
		t.append("\nTodas as medidas já estão dentro do alvo.")
	_prec_text.text = "".join(t)
	_prec_more.disabled = _prec_more_n <= 0
	_prec_more.text = "Rodar mais %d" % _prec_more_n if _prec_more_n > 0 else "Rodar mais"


func _run_more() -> void:
	if results == null or _prec_more_n <= 0:
		return
	var job: Object = results.start_more(_prec_more_n)
	_start(job, func(j):
		var res: Object = j.results()
		if res != null:
			main.on_results(res)
			_precision())


func _start_little() -> void:
	if results == null:
		return
	_start(results.start_little(), func(j): _show_little(j.data()))


func _light(name: String) -> String:
	match name:
		"verde":
			return "[color=#73d980]● verde[/color]"
		"amarelo":
			return "[color=#fac859]● amarelo[/color]"
	return "[color=#ff7368]● vermelho[/color]"


func _show_little(d: Variant) -> void:
	if d == null:
		return
	var t := PackedStringArray()
	t.append("[table=5]")
	for head in ["Objeto", "L", "λ·W", "Diferença", "Semáforo"]:
		t.append("[cell][b]%s[/b]  [/cell]" % head)
	var rows: Array = d["objects"].duplicate()
	if d.get("system") != null:
		var sys: Dictionary = d["system"].duplicate()
		sys["name"] = "sistema"
		rows.append(sys)
	var lights := {"verde": 0, "amarelo": 0, "vermelho": 0}
	for r in rows:
		t.append("[cell]%s  [/cell]" % r["name"])
		t.append("[cell]%s  [/cell]" % mh(r["l"]))
		t.append("[cell]%s  [/cell]" % mh(r["lambda_w"]))
		t.append("[cell]%s%%  [/cell]" % num(float(r["relative"]) * 100.0) if r["relative"] != null else "[cell]-[/cell]")
		t.append("[cell]%s[/cell]" % _light(r["light"]))
		lights[r["light"]] = int(lights[r["light"]]) + 1
	t.append("[/table]")
	_little_text.text = "".join(t)
	main.set_status("Lei de Little: %d verde(s), %d amarelo(s), %d vermelho(s)." % [lights["verde"], lights["amarelo"], lights["vermelho"]])


func _start_shifting() -> void:
	if results == null:
		return
	_start(results.start_shifting(), func(j): _show_shifting(j.data()))


func _show_shifting(d: Variant) -> void:
	if d == null:
		return
	var rows := []
	var first := ""
	for r in d:
		var sole = r["sole"].get("mean")
		var shared = r["shifting"].get("mean")
		if first == "":
			first = r["name"]
		rows.append({"label": r["name"], "value": float(r["total"]) * 100.0 if r["total"] != null else 0.0,
			"half_width": null, "hot": r["name"] == first})
		rows[-1]["label"] = "%s (único %s%%, compartilhado %s%%)" % [r["name"], num(float(sole) * 100.0) if sole != null else "-", num(float(shared) * 100.0) if shared != null else "-"]
	_shift_chart.bars(rows, "%", 100.0)
	_shift_text.text = "Gargalo móvel: %s na maior parte do tempo." % first if first != "" else "Nenhum recurso ativo."
	main.set_status(_shift_text.text)


# ---------------------------------------------------------------------------
# Cenários
# ---------------------------------------------------------------------------


func _build_scenarios() -> void:
	var v := _tab("Cenários")
	_explain(v, "Um cenário por bloco: [nome] e, embaixo, as mudanças (objeto.campo = valor em RON, ou run.campo). O modelo como está entra primeiro, como base. Todos usam as mesmas sementes (números aleatórios comuns), então a diferença para a base é pareada; em laranja, as diferenças significativas.")
	_sc_text = TextEdit.new()
	_sc_text.name = "Res_CenariosTexto"
	_sc_text.custom_minimum_size = Vector2(0, 140)
	_sc_text.placeholder_text = "[mais rápido]\nmaquina.process = Exp(mean: 2.0)\n\n[dois servidores]\nmaquina.capacity = 2"
	v.add_child(_sc_text)
	var h := _row(v)
	_label(h, "Medidas (vírgulas; vazio: todas):")
	_sc_measures = _line(h, "Res_CenariosMedidas", "", 280)
	_sc_measures.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_label(h, "Replicações:")
	_sc_reps = _spin(h, "Res_CenariosReplicacoes", 2, 1000, 5)
	_button(h, "Res_CenariosComparar", "Comparar", "Roda todos os cenários e compara", func(): _start_scenarios())
	_sc_result = _rich("Res_CenariosResultado")
	v.add_child(_sc_result)


func _start_scenarios() -> void:
	var model: Object = main.model
	if model == null or not model.is_loaded():
		main.set_status("Simule o modelo antes de comparar cenários.")
		return
	var measures := PackedStringArray()
	for m in _sc_measures.text.split(",", false):
		measures.append(m.strip_edges())
	var job: Object = model.start_scenarios(_sc_text.text, measures, int(_sc_reps.value))
	if job == null:
		_sc_result.text = "[color=#ff7a70]%s[/color]" % model.last_error()
		return
	main.start_job(job, func(j): _show_scenarios(j.data()))


func _show_scenarios(d: Variant) -> void:
	if d == null:
		return
	var names: Array = d["scenarios"]
	var t := PackedStringArray()
	t.append("[table=%d]" % (names.size() + 1))
	t.append("[cell][b]Medida[/b]  [/cell]")
	for n in names:
		t.append("[cell][b]%s[/b]  [/cell]" % n)
	for r in d["rows"]:
		t.append("[cell]%s  [/cell]" % r["metric"])
		for k in names.size():
			var cell := mh(r["values"][k])
			var diff = r["differences"][k]
			if diff != null:
				var dt := "Δ %s" % mh(diff)
				cell += "  " + ("[color=#ff9a3d]%s[/color]" % dt if diff.get("significant", false) else "[color=#aab2be]%s[/color]" % dt)
			t.append("[cell]%s  [/cell]" % cell)
	t.append("[/table]")
	_sc_result.text = "".join(t)
	main.set_status("%d cenário(s) comparados." % names.size())


# ---------------------------------------------------------------------------
# Otimização
# ---------------------------------------------------------------------------


func _build_optimize() -> void:
	var v := _tab("Otimização")
	_explain(v, "Procura os valores das variáveis que minimizam (ou maximizam) a média de uma métrica. Uma variável por linha: chave mínimo máximo [passo] (inteiros) ou com ponto decimal (contínua, cinco valores na grade). Os pontos não significativamente diferentes do melhor (teste pareado de 95 %) ficam marcados.")
	var h := _row(v)
	_label(h, "Objetivo:")
	_op_metric = _line(h, "Res_OtimMetrica", "", 300)
	_op_metric.placeholder_text = "saida.time_in_system.mean"
	_op_max = CheckBox.new()
	_op_max.name = "Res_OtimMaximizar"
	_op_max.text = "maximizar"
	h.add_child(_op_max)
	_op_vars = TextEdit.new()
	_op_vars.name = "Res_OtimVariaveis"
	_op_vars.custom_minimum_size = Vector2(0, 80)
	_op_vars.placeholder_text = "maquina.capacity 1 3"
	v.add_child(_op_vars)
	h = _row(v)
	_label(h, "Método:")
	_op_method = _option(h, "Res_OtimMetodo", ["grade", "recozimento", "genetico"])
	_label(h, "Replicações por ponto:")
	_op_reps = _spin(h, "Res_OtimReplicacoes", 2, 200, 5)
	_label(h, "Orçamento (pontos):")
	_op_budget = _spin(h, "Res_OtimOrcamento", 1, 5000, 50)
	_button(h, "Res_Otimizar", "Otimizar", "Roda a busca", func(): _start_optimize())
	_op_result = _rich("Res_OtimResultado")
	v.add_child(_op_result)
	_op_chart = _chart_box(v, "Res_OtimGrafico", 180)


func _start_optimize() -> void:
	var model: Object = main.model
	if model == null or not model.is_loaded():
		main.set_status("Simule o modelo antes de otimizar.")
		return
	var form := {
		"metric": _op_metric.text.strip_edges(),
		"maximize": _op_max.button_pressed,
		"variables": _op_vars.text,
		"method": _op_method.get_item_text(_op_method.selected),
		"replications": int(_op_reps.value),
		"budget": int(_op_budget.value),
	}
	var job: Object = model.start_optimize(form)
	if job == null:
		_op_result.text = "[color=#ff7a70]%s[/color]" % model.last_error()
		return
	main.start_job(job, func(j): _show_optimized(j.data()))


func _show_optimized(d: Variant) -> void:
	if d == null:
		return
	var t := PackedStringArray()
	var best = d.get("best")
	t.append("[b]Melhor ponto: %s[/b] (%d ponto(s) simulados)\n" % [best if best != null else "nenhum viável", int(d["simulated"])])
	for line in d.get("explanation", []):
		t.append("[color=#aab2be]%s[/color]\n" % line)
	t.append("[table=3]")
	for head in ["Ponto", "Objetivo", ""]:
		t.append("[cell][b]%s[/b]  [/cell]" % head)
	for e in d["evaluations"]:
		var mark := ""
		if e["best"]:
			mark = "[color=#ff9a3d]melhor[/color]"
		elif e["equivalent"]:
			mark = "equivalente"
		elif not e["feasible"]:
			mark = "inviável"
		t.append("[cell]%s  [/cell][cell]%s  [/cell][cell]%s[/cell]" % [e["point"], mh(e["objective"]), mark])
	t.append("[/table]")
	_op_result.text = "".join(t)
	var hist := []
	for p in d.get("history", []):
		hist.append([float(p[0]), float(p[1])])
	_op_chart.lines([{"points": hist, "color": Color(0.98, 0.80, 0.35), "width": 2.0}], {"x_label": "pontos avaliados", "y_label": "melhor objetivo"})
	main.set_status("Otimização: melhor ponto %s." % (best if best != null else "nenhum"))


## Plain text of the visible tab (scripted checks).
func tab_text() -> String:
	var parts := PackedStringArray()
	for r in [_summary, _table, _prec_text, _little_text, _sc_result, _op_result]:
		parts.append((r as RichTextLabel).get_parsed_text())
	for l in [_warm_text, _shift_text, _chart_note]:
		parts.append((l as Label).text)
	return "\n".join(parts)


func select_tab(title: String) -> bool:
	for k in _tabs.get_tab_count():
		if _tabs.get_tab_title(k) == title:
			_tabs.current_tab = k
			return true
	return false
