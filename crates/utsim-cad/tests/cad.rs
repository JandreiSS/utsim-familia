//! Testes da planta CAD com DXFs montados em texto no próprio teste.

use utsim_cad::*;

/// Monta um par (código, valor) por linha.
fn g(pairs: &[(i32, &str)]) -> String {
    pairs.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect()
}

fn dxf(insunits: Option<i32>, blocks: &str, entities: &str) -> String {
    let mut s = String::new();
    s += &g(&[(0, "SECTION"), (2, "HEADER")]);
    if let Some(u) = insunits {
        s += &g(&[(9, "$INSUNITS"), (70, &u.to_string())]);
    }
    s += &g(&[(0, "ENDSEC"), (0, "SECTION"), (2, "BLOCKS")]);
    s += blocks;
    s += &g(&[(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
    s += entities;
    s += &g(&[(0, "ENDSEC"), (0, "EOF")]);
    s
}

fn line(layer: &str, x0: f64, y0: f64, x1: f64, y1: f64) -> String {
    g(&[
        (0, "LINE"),
        (8, layer),
        (10, &x0.to_string()),
        (20, &y0.to_string()),
        (11, &x1.to_string()),
        (21, &y1.to_string()),
    ])
}

fn near(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

fn pt(a: Pt, b: Pt) {
    near(a[0], b[0]);
    near(a[1], b[1]);
}

#[test]
fn linha_e_camadas() {
    let s = dxf(
        Some(6),
        "",
        &(line("PAREDES", 0.0, 0.0, 10.0, 0.0) + &line("COTAS", 1.0, 1.0, 2.0, 1.0)),
    );
    let p = read_dxf(&s, &ReadOptions::default()).unwrap();
    assert_eq!(p.layers.len(), 2);
    assert_eq!(p.layers[0].name, "COTAS"); // ordem alfabética
    let w = p.layer("PAREDES").unwrap();
    assert_eq!(w.polylines[0].points, vec![[0.0, 0.0], [10.0, 0.0]]);
    assert!(!p.units_assumed);
    let sm = p.summaries();
    assert_eq!(sm[1].entities, 1);
    assert_eq!(sm[1].segments, 1);
    let bb = p.bbox().unwrap();
    assert_eq!(bb.min, [0.0, 0.0]);
    assert_eq!(bb.max, [10.0, 1.0]);
}

#[test]
fn unidades_do_cabecalho() {
    for (code, k) in [(4, 0.001), (5, 0.01), (6, 1.0), (1, 0.0254), (2, 0.3048)] {
        let s = dxf(Some(code), "", &line("A", 0.0, 0.0, 1000.0, 0.0));
        let p = read_dxf(&s, &ReadOptions::default()).unwrap();
        near(p.layers[0].polylines[0].points[1][0], 1000.0 * k);
        assert!(!p.units_assumed);
    }
    // sem unidade: assume milímetros e avisa
    let s = dxf(None, "", &line("A", 0.0, 0.0, 1000.0, 0.0));
    let p = read_dxf(&s, &ReadOptions::default()).unwrap();
    near(p.layers[0].polylines[0].points[1][0], 1.0);
    assert!(p.units_assumed);
    // unidade forçada pela pessoa
    let o = ReadOptions {
        units: Some(Units::Centimeters),
        ..Default::default()
    };
    let p = read_dxf(&s, &o).unwrap();
    near(p.layers[0].polylines[0].points[1][0], 10.0);
    // sem conversão
    let o = ReadOptions {
        units: Some(Units::Drawing),
        ..Default::default()
    };
    near(
        read_dxf(&s, &o).unwrap().layers[0].polylines[0].points[1][0],
        1000.0,
    );
}

#[test]
fn lwpolyline_fechada_e_aberta() {
    let e = g(&[
        (0, "LWPOLYLINE"),
        (8, "P"),
        (90, "4"),
        (70, "1"),
        (10, "0"),
        (20, "0"),
        (10, "4"),
        (20, "0"),
        (10, "4"),
        (20, "3"),
        (10, "0"),
        (20, "3"),
    ]);
    let p = read_dxf(&dxf(Some(6), "", &e), &ReadOptions::default()).unwrap();
    let pl = &p.layers[0].polylines[0];
    assert!(pl.closed);
    assert_eq!(pl.points.len(), 4);
    assert_eq!(pl.segments().len(), 4);
    near(pl.length(), 14.0);
}

#[test]
fn lwpolyline_com_bulge() {
    // semicírculo de (0,0) a (2,0) com bulge 1 (anti-horário: passa por baixo)
    let e = g(&[
        (0, "LWPOLYLINE"),
        (8, "P"),
        (70, "0"),
        (10, "0"),
        (20, "0"),
        (42, "1"),
        (10, "2"),
        (20, "0"),
    ]);
    let p = read_dxf(&dxf(Some(6), "", &e), &ReadOptions::default()).unwrap();
    let pl = &p.layers[0].polylines[0];
    pt(pl.points[0], [0.0, 0.0]);
    pt(*pl.points.last().unwrap(), [2.0, 0.0]);
    // todos os pontos ficam a distância 1 do centro (1, 0) e abaixo do eixo x
    for q in &pl.points {
        near(((q[0] - 1.0).powi(2) + q[1].powi(2)).sqrt(), 1.0);
        assert!(q[1] <= 1e-9);
    }
    // comprimento perto de pi
    assert!((pl.length() - std::f64::consts::PI).abs() < 0.01);
    // bulge negativo passa por cima
    let e = e.replace("42\n1\n", "42\n-1\n");
    let p = read_dxf(&dxf(Some(6), "", &e), &ReadOptions::default()).unwrap();
    assert!(
        p.layers[0].polylines[0]
            .points
            .iter()
            .all(|q| q[1] >= -1e-9)
    );
}

#[test]
fn polyline_antiga_com_vertices() {
    let mut e = g(&[(0, "POLYLINE"), (8, "PL"), (70, "1")]);
    for (x, y) in [("0", "0"), ("2", "0"), ("2", "2")] {
        e += &g(&[(0, "VERTEX"), (8, "PL"), (10, x), (20, y)]);
    }
    e += &g(&[(0, "SEQEND"), (8, "PL")]);
    e += &line("L", 0.0, 0.0, 1.0, 1.0);
    let p = read_dxf(&dxf(Some(6), "", &e), &ReadOptions::default()).unwrap();
    let pl = &p.layer("PL").unwrap().polylines[0];
    assert!(pl.closed);
    assert_eq!(pl.points.len(), 3);
    assert!(p.layer("L").is_some(), "a entidade após SEQEND é lida");
}

#[test]
fn circulo_e_arco() {
    let e = g(&[
        (0, "CIRCLE"),
        (8, "C"),
        (10, "5"),
        (20, "5"),
        (40, "2"),
        (0, "ARC"),
        (8, "A"),
        (10, "0"),
        (20, "0"),
        (40, "3"),
        (50, "0"),
        (51, "90"),
    ]);
    let p = read_dxf(&dxf(Some(6), "", &e), &ReadOptions::default()).unwrap();
    let c = &p.layer("C").unwrap().polylines[0];
    assert!(c.closed);
    for q in &c.points {
        near(((q[0] - 5.0).powi(2) + (q[1] - 5.0).powi(2)).sqrt(), 2.0);
    }
    let a = &p.layer("A").unwrap().polylines[0];
    assert!(!a.closed);
    pt(a.points[0], [3.0, 0.0]);
    pt(*a.points.last().unwrap(), [0.0, 3.0]);
    assert!((a.length() - 3.0 * std::f64::consts::FRAC_PI_2).abs() < 0.01);
    // arco que cruza 0 graus (330 a 30)
    let e = e.replace("50\n0\n51\n90\n", "50\n330\n51\n30\n");
    let p = read_dxf(&dxf(Some(6), "", &e), &ReadOptions::default()).unwrap();
    let a = &p.layer("A").unwrap().polylines[0];
    assert!((a.length() - 3.0 * std::f64::consts::FRAC_PI_3).abs() < 0.01);
}

#[test]
fn texto_e_mtext() {
    let e = g(&[
        (0, "TEXT"),
        (8, "T"),
        (10, "1"),
        (20, "2"),
        (40, "0.5"),
        (1, "Doca 1"),
        (0, "MTEXT"),
        (8, "T"),
        (10, "3"),
        (20, "4"),
        (40, "0.25"),
        (1, "{\\fArial;Linha A}\\PLinha B"),
    ]);
    let p = read_dxf(&dxf(Some(6), "", &e), &ReadOptions::default()).unwrap();
    let l = &p.layer("T").unwrap().labels;
    assert_eq!(l[0].text, "Doca 1");
    pt(l[0].pos, [1.0, 2.0]);
    assert_eq!(l[1].text, "Linha A\nLinha B");
    // sem rótulos
    let o = ReadOptions {
        labels: false,
        ..Default::default()
    };
    assert!(
        read_dxf(&dxf(Some(6), "", &e), &o)
            .unwrap()
            .layers
            .is_empty()
    );
}

fn block(name: &str, base: (f64, f64), ents: &str) -> String {
    g(&[
        (0, "BLOCK"),
        (8, "0"),
        (2, name),
        (70, "0"),
        (10, &base.0.to_string()),
        (20, &base.1.to_string()),
    ]) + ents
        + &g(&[(0, "ENDBLK"), (8, "0")])
}

fn insert(name: &str, layer: &str, pos: (f64, f64), sx: f64, sy: f64, rot: f64) -> String {
    g(&[
        (0, "INSERT"),
        (8, layer),
        (2, name),
        (10, &pos.0.to_string()),
        (20, &pos.1.to_string()),
        (41, &sx.to_string()),
        (42, &sy.to_string()),
        (50, &rot.to_string()),
    ])
}

#[test]
fn insert_com_rotacao_escala_e_base() {
    // bloco: linha de (1,0) a (3,0) na camada "0"; ponto base (1,0)
    let b = block("B", (1.0, 0.0), &line("0", 1.0, 0.0, 3.0, 0.0));
    let e = insert("B", "MOVEIS", (10.0, 5.0), 2.0, 2.0, 90.0);
    let p = read_dxf(&dxf(Some(6), &b, &e), &ReadOptions::default()).unwrap();
    // camada "0" herda a do INSERT
    assert!(p.layer("0").is_none());
    let pl = &p.layer("MOVEIS").unwrap().polylines[0];
    // (1,0)->base->(0,0)->(10,5); (3,0)->(2,0)*2=(4,0)->rot 90=(0,4)->(10,9)
    pt(pl.points[0], [10.0, 5.0]);
    pt(pl.points[1], [10.0, 9.0]);
}

#[test]
fn insert_espelhado() {
    let b = block("B", (0.0, 0.0), &line("0", 0.0, 0.0, 2.0, 1.0));
    let e = insert("B", "X", (0.0, 0.0), -1.0, 1.0, 0.0);
    let p = read_dxf(&dxf(Some(6), &b, &e), &ReadOptions::default()).unwrap();
    pt(p.layer("X").unwrap().polylines[0].points[1], [-2.0, 1.0]);
}

#[test]
fn blocos_aninhados() {
    // B2 contém uma linha (0,0)-(1,0); B1 contém B2 deslocado (2,0) e girado 90 graus;
    // a planta insere B1 em (10,10) com escala 3.
    let b2 = block("B2", (0.0, 0.0), &line("0", 0.0, 0.0, 1.0, 0.0));
    let b1 = block(
        "B1",
        (0.0, 0.0),
        &insert("B2", "0", (2.0, 0.0), 1.0, 1.0, 90.0),
    );
    let e = insert("B1", "ARM", (10.0, 10.0), 3.0, 3.0, 0.0);
    let p = read_dxf(&dxf(Some(6), &(b2 + &b1), &e), &ReadOptions::default()).unwrap();
    let pl = &p.layer("ARM").unwrap().polylines[0];
    // em B1: (2,0)->(2,1); depois escala 3 e deslocamento: (16,10)->(16,13)
    pt(pl.points[0], [16.0, 10.0]);
    pt(pl.points[1], [16.0, 13.0]);
}

#[test]
fn bloco_recursivo_nao_trava() {
    let b = block(
        "R",
        (0.0, 0.0),
        &insert("R", "0", (1.0, 0.0), 1.0, 1.0, 0.0),
    );
    let e = insert("R", "L", (0.0, 0.0), 1.0, 1.0, 0.0);
    let p = read_dxf(&dxf(Some(6), &b, &e), &ReadOptions::default()).unwrap();
    assert!(p.layers.is_empty());
}

#[test]
fn arquivo_invalido() {
    assert!(read_dxf("isto não é DXF", &ReadOptions::default()).is_err());
    assert!(read_dxf("0\nSECTION\n", &ReadOptions::default()).is_err());
    assert!(read_dxf("AutoCAD Binary DXF\r\n\x1a\0", &ReadOptions::default()).is_err());
}

#[test]
fn calibracao_por_dois_pontos() {
    // dois pontos a 40 000 unidades, distância real 40 m: escala 0,001
    let c = Calibration::new([1000.0, 2000.0], [41000.0, 2000.0], 40.0).unwrap();
    near(c.scale(), 0.001);
    let t = c.transform();
    pt(t.apply([41000.0, 2000.0]), [41.0, 2.0]);
    // âncora: p1 vai para (0, 0)
    let t = c.transform_anchored([0.0, 0.0]);
    pt(t.apply([1000.0, 2000.0]), [0.0, 0.0]);
    pt(t.apply([41000.0, 2000.0]), [40.0, 0.0]);
    assert!(Calibration::new([0.0, 0.0], [0.0, 0.0], 1.0).is_err());
    assert!(Calibration::new([0.0, 0.0], [1.0, 0.0], 0.0).is_err());
}

#[test]
fn calibracao_aplicada_a_planta() {
    let s = dxf(None, "", &line("A", 1000.0, 2000.0, 41000.0, 2000.0));
    let o = ReadOptions {
        units: Some(Units::Drawing),
        ..Default::default()
    };
    let p = read_dxf(&s, &o).unwrap();
    let c = Calibration::new([1000.0, 2000.0], [41000.0, 2000.0], 40.0).unwrap();
    let q = p.transformed(&c.transform_anchored([0.0, 0.0]));
    let pl = &q.layers[0].polylines[0];
    pt(pl.points[0], [0.0, 0.0]);
    pt(pl.points[1], [40.0, 0.0]);
    near(q.meters_per_unit, 0.001);
}

#[test]
fn similaridade_com_rotacao() {
    // p1=(0,0), p2=(10,0) viram q1=(5,5), q2=(5,25): escala 2, rotação 90
    let t = Calibration::similarity([0.0, 0.0], [10.0, 0.0], [5.0, 5.0], [5.0, 25.0]).unwrap();
    near(t.scale, 2.0);
    near(t.rotation_deg, 90.0);
    pt(t.apply([0.0, 0.0]), [5.0, 5.0]);
    pt(t.apply([10.0, 0.0]), [5.0, 25.0]);
    pt(t.apply([0.0, 1.0]), [3.0, 5.0]);
    // composição
    let u = Transform::origin_at([1.0, 1.0]).then(&t);
    pt(u.apply([1.0, 1.0]), t.apply([0.0, 0.0]));
}

#[test]
fn deslocamento_da_origem() {
    let s = dxf(Some(6), "", &line("A", 5.0, 5.0, 8.0, 9.0));
    let p = read_dxf(&s, &ReadOptions::default()).unwrap();
    let q = p.transformed(&Transform::origin_at([5.0, 5.0]));
    assert_eq!(
        q.layers[0].polylines[0].points,
        vec![[0.0, 0.0], [3.0, 4.0]]
    );
}

#[test]
fn simplificacao() {
    let pts: Vec<Pt> = (0..=100)
        .map(|i| [i as f64, if i % 2 == 0 { 0.0 } else { 0.01 }])
        .collect();
    let s = simplify_polyline(&pts, false, 0.05);
    assert_eq!(s, vec![[0.0, 0.0], [100.0, 0.0]]);
    // com tolerância menor que o ruído, nada some
    assert_eq!(simplify_polyline(&pts, false, 0.001).len(), 101);
    // anel fica com ao menos 3 pontos
    let ring = vec![[0.0, 0.0], [5.0, 0.0], [5.0, 5.0], [0.0, 5.0]];
    assert_eq!(simplify_polyline(&ring, true, 0.1).len(), 4);
    assert!(simplify_polyline(&ring, true, 100.0).len() >= 3);
}

#[test]
fn simplificacao_da_planta() {
    let mut e = g(&[(0, "CIRCLE"), (8, "C"), (10, "0"), (20, "0"), (40, "10")]);
    e += &line("L", 0.0, 0.0, 1.0, 0.0);
    let p = read_dxf(&dxf(Some(6), "", &e), &ReadOptions::default()).unwrap();
    let before = p.segment_count();
    let q = p.simplified(0.5);
    assert!(q.segment_count() < before);
}

#[test]
fn sugestao_de_papel() {
    assert_eq!(suggest_role("PAREDES"), Role::Parede);
    assert_eq!(suggest_role("A-WALL"), Role::Parede);
    assert_eq!(suggest_role("Alvenaria"), Role::Parede);
    assert_eq!(suggest_role("COLUNAS"), Role::Coluna);
    assert_eq!(suggest_role("S-PILARES"), Role::Coluna);
    assert_eq!(suggest_role("A-DOOR"), Role::Porta);
    assert_eq!(suggest_role("PORTAS"), Role::Porta);
    assert_eq!(suggest_role("COTAS"), Role::Ignorar);
    assert_eq!(suggest_role("A-ANNO-DIMS"), Role::Ignorar);
    assert_eq!(suggest_role("HACHURA"), Role::Ignorar);
    assert_eq!(suggest_role("Defpoints"), Role::Ignorar);
    assert_eq!(suggest_role("PAREDE_COTAS"), Role::Ignorar);
    assert_eq!(suggest_role("MOVEIS"), Role::Fundo);
    assert_eq!(suggest_role("0"), Role::Fundo);
    assert_eq!(Role::parse("Paredes"), Some(Role::Parede));
}

#[test]
fn sobrescrita_do_usuario() {
    let s = dxf(
        Some(6),
        "",
        &(line("PAREDES", 0.0, 0.0, 1.0, 0.0) + &line("XYZ", 0.0, 0.0, 0.0, 1.0)),
    );
    let p = read_dxf(&s, &ReadOptions::default()).unwrap();
    let mut r = RoleMap::suggest(&p);
    assert_eq!(r.role_of("PAREDES"), Role::Parede);
    assert_eq!(r.role_of("XYZ"), Role::Fundo);
    r.set("XYZ", Role::Parede);
    r.set("PAREDES", Role::Ignorar);
    assert_eq!(r.role_of("XYZ"), Role::Parede);
    assert_eq!(r.role_of("PAREDES"), Role::Ignorar);
    assert_eq!(r.role_of("OUTRA_COLUNA"), Role::Coluna); // desconhecida: pelo nome
}

fn area(poly: &[Pt]) -> f64 {
    let n = poly.len();
    (0..n)
        .map(|i| poly[i][0] * poly[(i + 1) % n][1] - poly[(i + 1) % n][0] * poly[i][1])
        .sum::<f64>()
        / 2.0
}

#[test]
fn obstaculos_de_paredes_e_colunas() {
    // parede aberta de 10 m; coluna fechada 1 x 1; porta; fundo
    let col = g(&[
        (0, "LWPOLYLINE"),
        (8, "COLUNAS"),
        (70, "1"),
        (10, "5"),
        (20, "5"),
        (10, "5"),
        (20, "6"),
        (10, "6"),
        (20, "6"),
        (10, "6"),
        (20, "5"),
    ]);
    let e = line("PAREDES", 0.0, 0.0, 10.0, 0.0)
        + &col
        + &line("PORTAS", 4.0, 0.0, 5.0, 0.0)
        + &line("MOVEIS", 1.0, 1.0, 2.0, 2.0);
    let p = read_dxf(&dxf(Some(6), "", &e), &ReadOptions::default()).unwrap();
    let r = RoleMap::suggest(&p);
    let o = build_obstacles(&p, &r, 0.15);
    assert_eq!(o.polygons.len(), 2);
    let mut areas: Vec<f64> = o.polygons.iter().map(|q| area(q)).collect();
    areas.sort_by(f64::total_cmp);
    near(areas[0], 1.0); // coluna
    near(areas[1], 10.0 * 0.15); // parede: comprimento x espessura (anti-horário: positiva)
    assert_eq!(o.wall_centerlines.len(), 1);
    assert_eq!(o.openings, vec![[[4.0, 0.0], [5.0, 0.0]]]);
}

#[test]
fn parede_fechada_por_pontas_coincidentes() {
    // anel de paredes feito de uma polilinha aberta que volta ao início
    let e = g(&[
        (0, "LWPOLYLINE"),
        (8, "PAREDES"),
        (70, "0"),
        (10, "0"),
        (20, "0"),
        (10, "4"),
        (20, "0"),
        (10, "4"),
        (20, "3"),
        (10, "0"),
        (20, "3"),
        (10, "0"),
        (20, "0"),
    ]);
    let p = read_dxf(&dxf(Some(6), "", &e), &ReadOptions::default()).unwrap();
    let o = build_obstacles(&p, &RoleMap::suggest(&p), 0.15);
    assert_eq!(o.polygons.len(), 1);
    near(area(&o.polygons[0]), 12.0);
}

#[test]
fn exportacao_plana() {
    let s = dxf(
        Some(6),
        "",
        &(line("A", 0.0, 0.0, 1.0, 0.0) + &line("A", 1.0, 1.0, 2.0, 3.0)),
    );
    let p = read_dxf(&s, &ReadOptions::default()).unwrap();
    let f = FlatLines::from_layer(&p.layers[0]);
    assert_eq!(f.starts, vec![0, 2, 4]);
    assert_eq!(f.coords, vec![0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 2.0, 3.0]);
    assert_eq!(f.segments.len(), 8);
    assert_eq!(f.closed, vec![false, false]);
}

#[test]
fn svg_deterministico() {
    let s = dxf(
        Some(6),
        "",
        &(line("PAREDES", 0.0, 0.0, 10.0, 0.0) + &line("COTAS", 0.0, 1.0, 10.0, 1.0)),
    );
    let p = read_dxf(&s, &ReadOptions::default()).unwrap();
    let o = SvgOptions {
        roles: Some(RoleMap::suggest(&p)),
        ..Default::default()
    };
    let a = to_svg(&p, &o);
    assert_eq!(a, to_svg(&p, &o));
    assert!(a.starts_with("<svg"));
    assert!(a.contains("id=\"PAREDES\""));
    assert!(
        !a.contains("id=\"COTAS\""),
        "camada ignorada não é desenhada"
    );
    // com lista explícita, a camada pedida aparece
    let o2 = SvgOptions {
        layers: vec!["COTAS".into()],
        ..o
    };
    assert!(to_svg(&p, &o2).contains("id=\"COTAS\""));
}

#[test]
fn definicao_da_planta_em_ron() {
    let d = FloorPlanDef {
        file: "planta/galpao.dxf".into(),
        hash: fnv1a_hash(b"abc"),
        transform: Transform::IDENTITY,
        calibration: Some(Calibration::new([0.0, 0.0], [1000.0, 0.0], 1.0).unwrap()),
        layers: vec![FloorPlanLayer {
            name: "PAREDES".into(),
            role: Role::Parede,
            visible: true,
        }],
        wall_thickness: 0.15,
        origin: [0.0, 0.0],
    };
    let txt = ron::to_string(&d).unwrap();
    let back: FloorPlanDef = ron::from_str(&txt).unwrap();
    assert_eq!(back, d);
    // campos opcionais podem faltar
    let min: FloorPlanDef =
        ron::from_str("(file: \"a.dxf\", hash: \"x\", layers: [(name: \"P\", role: Parede)])")
            .unwrap();
    assert_eq!(min.wall_thickness, 0.15);
    assert!(min.layers[0].visible);
    // o hash muda quando o conteúdo muda
    assert_ne!(fnv1a_hash(b"abc"), fnv1a_hash(b"abd"));
    assert_eq!(fnv1a_hash(b"abc"), fnv1a_hash(b"abc"));
}
