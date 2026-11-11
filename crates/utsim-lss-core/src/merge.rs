//! Merge of two edits of one long text by line (P7, "conflito por parágrafo"). Two people
//! changed the same text from the same version: when they touched different lines, both
//! changes are kept; when they touched the same lines (or both added lines at the same
//! place), there is no merge and the field becomes a conflict, as any other field.

/// Above this many line comparisons the merge is not tried (the texts of a project are a few
/// hundred lines at most; this only keeps a pasted book from stalling the reconstruction).
const MAX_CELLS: usize = 4_000_000;

/// One change of a side against the common version: the lines `start..end` of the common
/// version replaced by `lines`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Hunk<'a> {
    start: usize,
    end: usize,
    lines: Vec<&'a str>,
}

/// The changes turning `base` into `other`, by the longest common sequence of lines.
fn hunks<'a>(base: &[&'a str], other: &[&'a str]) -> Option<Vec<Hunk<'a>>> {
    let (n, m) = (base.len(), other.len());
    if (n + 1).saturating_mul(m + 1) > MAX_CELLS {
        return None;
    }
    // lcs[i][j]: length of the longest common sequence of base[i..] and other[j..].
    let w = m + 1;
    let mut lcs = vec![0u32; (n + 1) * w];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            lcs[i * w + j] = if base[i] == other[j] {
                lcs[(i + 1) * w + j + 1] + 1
            } else {
                lcs[(i + 1) * w + j].max(lcs[i * w + j + 1])
            };
        }
    }
    let mut out = Vec::new();
    let mut open: Option<Hunk<'a>> = None;
    let (mut i, mut j) = (0, 0);
    while i < n || j < m {
        if i < n && j < m && base[i] == other[j] {
            if let Some(h) = open.take() {
                out.push(h);
            }
            i += 1;
            j += 1;
            continue;
        }
        let h = open.get_or_insert(Hunk {
            start: i,
            end: i,
            lines: Vec::new(),
        });
        if j >= m || (i < n && lcs[(i + 1) * w + j] >= lcs[i * w + j + 1]) {
            i += 1;
            h.end = i;
        } else {
            h.lines.push(other[j]);
            j += 1;
        }
    }
    out.extend(open);
    Some(out)
}

/// Whether two changes touch the same lines: their ranges cross, or they start at the same
/// line and one of them only adds lines there (the order of the additions would be a guess).
fn clash(a: &Hunk, b: &Hunk) -> bool {
    (a.start < b.end && b.start < a.end)
        || (a.start == b.start && (a.start == a.end || b.start == b.end))
}

/// The text with the changes of `a` and of `b` over `base`, or `None` when they touch the
/// same lines. Lines are split at `\n`; the result keeps the same separators.
pub fn merge_lines(base: &str, a: &str, b: &str) -> Option<String> {
    if a == b {
        return Some(a.to_string());
    }
    let base_l: Vec<&str> = base.split('\n').collect();
    let ha = hunks(&base_l, &a.split('\n').collect::<Vec<_>>())?;
    let hb = hunks(&base_l, &b.split('\n').collect::<Vec<_>>())?;
    let mut out: Vec<&str> = Vec::new();
    let (mut ia, mut ib, mut pos) = (0, 0, 0);
    loop {
        let h = match (ha.get(ia), hb.get(ib)) {
            (None, None) => break,
            (Some(x), Some(y)) if x == y => {
                ia += 1;
                ib += 1;
                x
            }
            (Some(x), Some(y)) if clash(x, y) => return None,
            (Some(x), Some(y)) if x.start <= y.start => {
                ia += 1;
                x
            }
            (_, Some(y)) => {
                ib += 1;
                y
            }
            (Some(x), None) => {
                ia += 1;
                x
            }
        };
        out.extend_from_slice(&base_l[pos..h.start]);
        out.extend_from_slice(&h.lines);
        pos = h.end;
    }
    out.extend_from_slice(&base_l[pos..]);
    Some(out.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "Abertura.\nPresentes: Ana, Bruno.\nPauta: FMEA.\nEncerramento.";

    #[test]
    fn different_lines_are_both_kept() {
        let a = "Abertura às 9h.\nPresentes: Ana, Bruno.\nPauta: FMEA.\nEncerramento.";
        let b =
            "Abertura.\nPresentes: Ana, Bruno.\nPauta: FMEA e plano de controle.\nEncerramento.";
        assert_eq!(
            merge_lines(BASE, a, b).as_deref(),
            Some(
                "Abertura às 9h.\nPresentes: Ana, Bruno.\nPauta: FMEA e plano de controle.\nEncerramento."
            )
        );
        // The order of the sides does not change the result.
        assert_eq!(merge_lines(BASE, a, b), merge_lines(BASE, b, a));
    }

    #[test]
    fn additions_and_removals_in_different_places() {
        let a = "Abertura.\nPresentes: Ana, Bruno.\nPauta: FMEA.\nAção: revisar S.\nEncerramento.";
        let b = "Presentes: Ana, Bruno.\nPauta: FMEA.\nEncerramento.";
        assert_eq!(
            merge_lines(BASE, a, b).as_deref(),
            Some("Presentes: Ana, Bruno.\nPauta: FMEA.\nAção: revisar S.\nEncerramento.")
        );
    }

    #[test]
    fn same_line_or_same_place_is_a_conflict() {
        let a = "Abertura.\nPresentes: Ana.\nPauta: FMEA.\nEncerramento.";
        let b = "Abertura.\nPresentes: Ana, Bruno, Carla.\nPauta: FMEA.\nEncerramento.";
        assert_eq!(merge_lines(BASE, a, b), None);
        let a = "Abertura.\nX\nPresentes: Ana, Bruno.\nPauta: FMEA.\nEncerramento.";
        let b = "Abertura.\nY\nPresentes: Ana, Bruno.\nPauta: FMEA.\nEncerramento.";
        assert_eq!(merge_lines(BASE, a, b), None);
        // The same change on both sides is no conflict.
        assert_eq!(merge_lines(BASE, a, a).as_deref(), Some(a));
    }

    #[test]
    fn a_one_line_text_grows_on_both_ends() {
        assert_eq!(
            merge_lines("meio", "começo\nmeio", "meio\nfim").as_deref(),
            Some("começo\nmeio\nfim")
        );
    }
}
