## The PERT/CPM window of the selection (phase G3, `docs/plano_analises_e_dinamica.md`,
## section 1). It opens in the **simulated** mode (the critical path observed in the
## simulation itself, with queues, transporters and resources: the owner's choice); the
## classic PERT and the Monte Carlo over the network are one click away.
##
## Tabs: **Rede** (activity on node, boxes with ES/EF/LS/LF and the float, the critical
## path in red, arrows as thick as the criticality), **Gantt** (ES→EF, the float as a
## light bar), **Distribuição** (histogram of the total time with the deadline and
## `P(T ≤ prazo)`), **Tabela** and **Texto** (the report of `utsim pert`). The critical
## objects get a red outline in the 3D view (`EditorUtsim.critical`).
##
## Every number and coordinate comes from the bridge (`UtsimPert.data()`, with the
## layout of `utsim_pert::report::diagram`); this script only draws.
class_name PainelPert
extends PanelContainer

const HOT := Color(1.0, 0.42, 0.30)
const BOX := Color(0.16, 0.18, 0.22)
const LINE := Color(0.55, 0.60, 0.68)
const TEXT := Color(0.88, 0.90, 0.95)
const BAR := Color(0.35, 0.59, 0.94)

var main: Node
## The selection the window was opened for, and whether it is "from A to B".
var names := PackedStringArray()
var between := false
var mode := "simulado"
## `UtsimPert.data()` of the last calculation.
var data: Dictionary = {}
var pert: Object
var _title: Label
var _info: RichTextLabel
var _deadline: LineEdit
var _samples: SpinBox
var _modes := {}
var _tabs: TabContainer
var _diagram: Diagrama
var _gantt: Gantt
var _hist: Grafico
var _hist_text: Label
var _table: RichTextLabel
var _text: TextEdit
var _file: LineEdit


func build(owner_main: Node) -> void:
	main = owner_main
	name = "PainelPert"
	var root := VBoxContainer.new()
	add_child(root)
	var head := HBoxContainer.new()
	root.add_child(head)
	_title = Label.new()
	_title.text = "PERT/CPM"
	_title.theme_type_variation = &"TituloPainel"
	_title.clip_text = true
	_title.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	head.add_child(_title)
	var group := ButtonGroup.new()
	for m in [["simulado", "Simulado", "Caminho crítico observado na simulação: filas, transportes e recursos incluídos"],
			["classico", "Clássico", "PERT clássico: estimativas (a, m, b), CPM nas médias e P(prazo) pela normal"],
			["montecarlo", "Monte Carlo", "Sorteios das durações sobre a rede: criticidade e distribuição do tempo total"]]:
		var b := _button(head, "Pert_Modo_" + m[0], m[1], m[2], func(): calculate(m[0]))
		b.toggle_mode = true
		b.button_group = group
		_modes[m[0]] = b
	_button(head, "Pert_Fechar", "Fechar", "Fechar a janela PERT/CPM", func(): close())
	var opts := HBoxContainer.new()
	root.add_child(opts)
	var l := Label.new()
	l.text = "Prazo:"
	opts.add_child(l)
	_deadline = LineEdit.new()
	_deadline.name = "Pert_Prazo"
	_deadline.placeholder_text = "sem prazo"
	_deadline.custom_minimum_size = Vector2(100, 0)
	_deadline.text_submitted.connect(func(_t): calculate(mode))
	opts.add_child(_deadline)
	l = Label.new()
	l.text = "Sorteios (Monte Carlo):"
	opts.add_child(l)
	_samples = SpinBox.new()
	_samples.name = "Pert_Sorteios"
	_samples.min_value = 100
	_samples.max_value = 1000000
	_samples.step = 100
	_samples.value = 10000
	opts.add_child(_samples)
	_button(opts, "Pert_Recalcular", "Recalcular", "Calcula de novo no modo atual", func(): calculate(mode))
	var sp := Control.new()
	sp.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	opts.add_child(sp)
	_file = LineEdit.new()
	_file.name = "Pert_Arquivo"
	_file.custom_minimum_size = Vector2(260, 0)
	_file.placeholder_text = "relatório .html"
	opts.add_child(_file)
	_button(opts, "Pert_ExportarHTML", "Exportar HTML", "Grava o relatório HTML do PERT/CPM", func(): _export_html())
	_button(opts, "Pert_ExportarCSV", "CSV", "Grava as tabelas em CSV na pasta do arquivo", func(): _export_csv())
	_info = RichTextLabel.new()
	_info.name = "Pert_Resumo"
	_info.bbcode_enabled = true
	_info.fit_content = true
	_info.selection_enabled = true
	root.add_child(_info)
	_tabs = TabContainer.new()
	_tabs.name = "Pert_Abas"
	_tabs.size_flags_vertical = Control.SIZE_EXPAND_FILL
	root.add_child(_tabs)
	var scroll := _tab("Rede")
	_diagram = Diagrama.new()
	_diagram.name = "Pert_Diagrama"
	scroll.add_child(_diagram)
	scroll = _tab("Gantt")
	_gantt = Gantt.new()
	_gantt.name = "Pert_Gantt"
	scroll.add_child(_gantt)
	scroll = _tab("Distribuição")
	var v := VBoxContainer.new()
	v.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(v)
	_hist_text = Label.new()
	_hist_text.name = "Pert_DistribuicaoTexto"
	_hist_text.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	v.add_child(_hist_text)
	_hist = Grafico.new()
	_hist.name = "Pert_Histograma"
	_hist.custom_minimum_size = Vector2(0, 300)
	_hist.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	v.add_child(_hist)
	scroll = _tab("Tabela")
	_table = RichTextLabel.new()
	_table.name = "Pert_Tabela"
	_table.bbcode_enabled = true
	_table.fit_content = true
	_table.selection_enabled = true
	_table.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	scroll.add_child(_table)
	_text = TextEdit.new()
	_text.name = "Pert_Texto"
	_text.editable = false
	var mono := SystemFont.new()
	mono.font_names = PackedStringArray(["Consolas", "DejaVu Sans Mono", "monospace"])
	_text.add_theme_font_override("font", mono)
	_tabs.add_child(_text)
	_tabs.set_tab_title(_tabs.get_tab_count() - 1, "Texto")


func _button(parent: Node, id: String, text: String, tip: String, action: Callable) -> Button:
	var b := Button.new()
	b.name = id
	b.text = text
	b.tooltip_text = tip
	b.focus_mode = Control.FOCUS_NONE
	b.pressed.connect(action)
	parent.add_child(b)
	return b


func _tab(title: String) -> ScrollContainer:
	var s := ScrollContainer.new()
	s.name = "Pert_Aba_" + title
	_tabs.add_child(s)
	_tabs.set_tab_title(_tabs.get_tab_count() - 1, title)
	return s


## Opens the window for `selection` (or from its first to its second name) and starts
## the simulated mode.
func open(selection: PackedStringArray, from_to: bool) -> void:
	names = selection
	between = from_to
	data = {}
	pert = null
	visible = true
	var what := ("de %s até %s" % [selection[0], selection[1]]) if from_to and selection.size() >= 2 else ", ".join(selection)
	_title.text = "PERT/CPM: %s" % what
	calculate("simulado")


func close() -> void:
	visible = false
	if main.editor != null:
		main.editor.critical = []


func calculate(which: String) -> void:
	mode = which
	(_modes[which] as Button).set_pressed_no_signal(true)
	var model: Object = main.document_model()
	if model == null:
		_info.text = "[color=#ff7a70]O modelo tem problemas e não roda: corrija no Editar.[/color]"
		return
	var deadline := float(_deadline.text.replace(",", ".")) if _deadline.text.strip_edges() != "" else 0.0
	var job: Object = model.start_pert(names, between, which, deadline, int(_samples.value))
	if job == null:
		_info.text = "[color=#ff7a70]%s[/color]" % model.last_error()
		return
	_info.text = "Calculando (%s)..." % which
	main.start_job(job, func(j):
		var p: Object = j.pert()
		if p != null:
			show_pert(p))


func show_pert(p: Object) -> void:
	pert = p
	data = p.data()
	var t: Dictionary = data["total"]
	var lines := PackedStringArray()
	lines.append("[b]%s[/b] (%s)." % [data["mode_label"], data["selection"]])
	var units = t.get("units")
	var spread := ""
	if t.get("sd") != null:
		spread = ", desvio %s" % PainelResultados.num(float(t["sd"]))
	lines.append("Tempo total: [b]%s[/b]%s%s. CPM nas médias: %s." % [
		PainelResultados.mh(t), spread, (" (%s unidades/sorteios)" % str(units)) if units != null else "",
		PainelResultados.num(float(data["project"]))])
	var dl = t.get("deadline")
	if dl != null:
		var p_text := "%s%%" % PainelResultados.num(float(dl["probability"]) * 100.0)
		if dl.get("low") != null and dl.get("high") != null:
			p_text += " (IC %s%% a %s%%)" % [PainelResultados.num(float(dl["low"]) * 100.0), PainelResultados.num(float(dl["high"]) * 100.0)]
		lines.append("P(T ≤ %s) = [b]%s[/b]" % [PainelResultados.num(float(dl["deadline"])), p_text])
	var paths: Array = data.get("paths", [])
	if not paths.is_empty():
		lines.append("Caminho crítico: [color=#ff6b4d]%s[/color] (%s%%)" % [" → ".join(paths[0]["names"]), PainelResultados.num(float(paths[0]["index"]) * 100.0)])
	for w in data.get("warnings", []):
		lines.append("[color=#fac859]aviso: %s[/color]" % w)
	_info.text = "\n".join(lines)
	_diagram.display(data["diagram"])
	_gantt.display(data["activities"], float(data["project"]))
	var marker := float(dl["deadline"]) if dl != null else NAN
	_hist.histogram(data.get("histogram"), marker, "prazo" if dl != null else "")
	_hist_text.text = "Distribuição do tempo total (%s)%s." % [
		"aproximação normal do PERT clássico" if mode == "classico" else ("sorteios" if mode == "montecarlo" else "unidades observadas"),
		"; a linha laranja é o prazo" if dl != null else ""]
	var tb := PackedStringArray()
	tb.append("[table=9]")
	for head in ["Atividade", "Duração", "ES", "EF", "LS", "LF", "Folga total", "Folga livre", "Criticidade"]:
		tb.append("[cell][b]%s[/b]  [/cell]" % head)
	for a in data["activities"]:
		var name: String = a["name"]
		if a["critical"]:
			name = "[color=#ff6b4d]%s[/color]" % name
		tb.append("[cell]%s  [/cell]" % name)
		for k in ["mean", "es", "ef", "ls", "lf", "total_float", "free_float"]:
			tb.append("[cell]%s  [/cell]" % (PainelResultados.num(float(a[k])) if a[k] != null else "-"))
		tb.append("[cell]%s%%[/cell]" % PainelResultados.num(float(a["criticality"]) * 100.0) if a["criticality"] != null else "[cell]-[/cell]")
	tb.append("[/table]")
	for h in data.get("assumptions", []):
		tb.append("\n[color=#aab2be]hipótese: %s[/color]" % h)
	_table.text = "".join(tb)
	_text.text = p.text()
	if _file.text == "":
		var base: String = main.model_path.get_basename() if main.model_path != "" else OS.get_user_data_dir().path_join("pert")
		_file.text = base + "_pert.html"
	if main.editor != null:
		main.editor.critical = Array(data.get("critical_objects", []))
	main.set_status("PERT/CPM (%s): tempo total %s." % [mode, PainelResultados.mh(t)])


func _export_html() -> void:
	if pert == null:
		return
	var path := _file.text.strip_edges()
	if pert.save_html(path):
		main.set_status("Relatório PERT/CPM gravado em %s." % path)
	else:
		main.set_status("Não gravou %s." % path)


func _export_csv() -> void:
	if pert == null:
		return
	var dir := _file.text.strip_edges().get_base_dir()
	var files: PackedStringArray = pert.save_csv(dir)
	main.set_status("%d tabela(s) CSV em %s." % [files.size(), dir] if not files.is_empty() else "Não gravou os CSV em %s." % dir)


## Activity-on-node diagram: the boxes and arrows laid out by the bridge.
class Diagrama extends Control:
	var d: Dictionary = {}
	const PAD := 10.0

	func display(diagram: Dictionary) -> void:
		d = diagram
		custom_minimum_size = Vector2(float(d.get("width", 300)) + 2 * PAD, float(d.get("height", 200)) + 2 * PAD)
		queue_redraw()

	func boxes() -> int:
		return (d.get("boxes", []) as Array).size()

	func _draw() -> void:
		draw_rect(Rect2(Vector2.ZERO, size), Color(0.09, 0.10, 0.12))
		if d.is_empty():
			return
		var font := ThemeDB.fallback_font
		var o := Vector2(PAD, PAD)
		for a in d.get("arrows", []):
			var pts: Array = a["points"]
			var p := o + Vector2(float(pts[0][0]), float(pts[0][1]))
			var q := o + Vector2(float(pts[-1][0]), float(pts[-1][1]))
			var w := 1.0 + 4.0 * (float(a["weight"]) if a["weight"] != null else 0.0)
			var col: Color = PainelPert.HOT if a["critical"] else PainelPert.LINE
			draw_line(p, q, col, w, true)
			var dir := (q - p).normalized()
			var side := Vector2(-dir.y, dir.x) * 5.0
			draw_colored_polygon(PackedVector2Array([q, q - dir * 10 + side, q - dir * 10 - side]), col)
		for b in d.get("boxes", []):
			var r := Rect2(o + Vector2(float(b["x"]), float(b["y"])), Vector2(float(b["width"]), float(b["height"])))
			draw_rect(r, PainelPert.BOX)
			draw_rect(r, PainelPert.HOT if b["critical"] else PainelPert.LINE, false, 2.5 if b["critical"] else 1.0)
			draw_string(font, r.position + Vector2(6, 16), String(b["name"]), HORIZONTAL_ALIGNMENT_LEFT, r.size.x - 12, 13, PainelPert.TEXT)
			if b["milestone"]:
				continue
			var f := func(k: String) -> String:
				return PainelResultados.num(float(b[k])) if b[k] != null else "-"
			draw_string(font, r.position + Vector2(6, 34), "ES %s  EF %s" % [f.call("es"), f.call("ef")], HORIZONTAL_ALIGNMENT_LEFT, r.size.x - 12, 11, PainelPert.LINE)
			draw_string(font, r.position + Vector2(6, 48), "LS %s  LF %s" % [f.call("ls"), f.call("lf")], HORIZONTAL_ALIGNMENT_LEFT, r.size.x - 12, 11, PainelPert.LINE)
			var crit := float(b["criticality"]) if b["criticality"] != null else 0.0
			draw_string(font, r.position + Vector2(6, 61), "folga %s  crít. %d%%" % [f.call("total_float"), roundi(crit * 100.0)], HORIZONTAL_ALIGNMENT_LEFT, r.size.x - 12, 11, PainelPert.HOT if b["critical"] else PainelPert.LINE)


## Gantt of the CPM on the means: ES→EF, and the total float after it.
class Gantt extends Control:
	var rows: Array = []
	var project := 1.0
	const LEFT := 190.0
	const ROW := 24.0

	func display(activities: Array, total: float) -> void:
		rows = activities
		project = maxf(total, 1e-9)
		custom_minimum_size = Vector2(720, ROW * rows.size() + 50)
		queue_redraw()

	func _draw() -> void:
		draw_rect(Rect2(Vector2.ZERO, size), Color(0.09, 0.10, 0.12))
		var font := ThemeDB.fallback_font
		var w := maxf(size.x - LEFT - 20.0, 100.0)
		var k := w / project
		for i in rows.size():
			var a: Dictionary = rows[i]
			var y := 10.0 + i * ROW
			draw_string(font, Vector2(8, y + 15), String(a["name"]), HORIZONTAL_ALIGNMENT_LEFT, LEFT - 14, 12, PainelPert.TEXT)
			var es := float(a["es"]) if a["es"] != null else 0.0
			var ef := float(a["ef"]) if a["ef"] != null else es
			var tf := float(a["total_float"]) if a["total_float"] != null else 0.0
			draw_rect(Rect2(LEFT + es * k, y + 4, maxf((ef - es) * k, 1.5), 14), PainelPert.HOT if a["critical"] else PainelPert.BAR)
			if tf > 1e-9:
				draw_rect(Rect2(LEFT + ef * k, y + 8, tf * k, 6), Color(PainelPert.BAR, 0.3))
		var base := 14.0 + rows.size() * ROW
		draw_line(Vector2(LEFT, base), Vector2(LEFT + w, base), PainelPert.LINE)
		for j in 5:
			var x := LEFT + w * j / 4.0
			draw_string(font, Vector2(x - 16, base + 16), PainelResultados.num(project * j / 4.0), HORIZONTAL_ALIGNMENT_LEFT, -1, 11, PainelPert.LINE)
