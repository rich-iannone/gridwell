//! Column and table widths.
//!
//! A column's width is absolute (`px`, `pt`, `em`, …: itself), a percentage (that
//! share of the table's width), a fraction (`fr`: a share of what the other columns
//! leave, in proportion to its number) or none (`auto`). The table's width is
//! `config.table_width`: an absolute length, or a percentage of the line; without
//! one, the table's width is the line's, which `%` and `fr` columns divide.
//!
//! When the table has a width and no column is `fr`, the columns without a width
//! share it as `1fr` each, so the table is as wide as it says (as CSS widens a
//! table's `auto` columns to its `width`). A table whose columns all have
//! absolute or `%` widths is as wide as they add up to.

use gridwell_core::Length;

use crate::ResolvedTable;

/// A column's width, as declared.
#[derive(Debug, Clone, PartialEq)]
pub enum ColumnWidth {
    /// No width: `auto`, or a width that isn't above zero.
    Auto,
    /// An absolute length, above zero.
    Fixed(Length),
    /// A share of the table's width, in percent, above zero.
    Percent(f64),
    /// A share of what the other columns leave, above zero.
    Fr(f64),
}

impl ColumnWidth {
    fn of(width: Option<&Length>) -> Self {
        let positive = |v: f64| v.is_finite() && v > 0.0;
        match width {
            Some(&Length::Percent(p)) if positive(p) => ColumnWidth::Percent(p),
            Some(&Length::Fr(f)) if positive(f) => ColumnWidth::Fr(f),
            Some(Length::Percent(_) | Length::Fr(_) | Length::Auto) | None => ColumnWidth::Auto,
            Some(l) if l.value().is_some_and(positive) => ColumnWidth::Fixed(l.clone()),
            Some(_) => ColumnWidth::Auto,
        }
    }
}

impl ResolvedTable<'_> {
    /// `config.table_width` when it gives the table a width: an absolute length or
    /// a percentage, above zero (`auto` and unparseable values give none).
    pub fn table_width(&self) -> Option<Length> {
        let width: Length = self.table.config.table_width.as_deref()?.parse().ok()?;
        match ColumnWidth::of(Some(&width)) {
            ColumnWidth::Fixed(l) => Some(l),
            ColumnWidth::Percent(p) => Some(Length::Percent(p)),
            ColumnWidth::Auto | ColumnWidth::Fr(_) => None,
        }
    }

    /// Each visible column's width. When the table has a width and no column is
    /// `fr`, the columns without a width are `1fr`.
    pub fn column_widths(&self) -> Vec<ColumnWidth> {
        let mut widths: Vec<ColumnWidth> = self
            .columns
            .iter()
            .map(|c| ColumnWidth::of(c.width.as_ref()))
            .collect();
        let fills =
            self.table_width().is_some() && !widths.iter().any(|w| matches!(w, ColumnWidth::Fr(_)));
        if fills {
            for w in &mut widths {
                if *w == ColumnWidth::Auto {
                    *w = ColumnWidth::Fr(1.0);
                }
            }
        }
        widths
    }

    /// Each visible column's width in points, for a format that gives every column
    /// a width of its own. `line_pt` is the width of the line: a `%` table width is
    /// its share of it, and without a table width it is the table's width, which
    /// `fr` columns fill. A column without a width is `auto_pt` wide, as is each
    /// `fr` column when the other columns leave nothing. `em` and `rem` are
    /// `em_pt`.
    pub fn column_widths_pt(&self, line_pt: f64, auto_pt: f64, em_pt: f64) -> Vec<f64> {
        let table_pt = match self.table_width() {
            Some(Length::Percent(p)) => p / 100.0 * line_pt,
            Some(l) => l.to_pt(em_pt, em_pt).unwrap_or(line_pt),
            None => line_pt,
        };
        let widths = self.column_widths();
        let sized: Vec<Option<f64>> = widths
            .iter()
            .map(|w| match w {
                ColumnWidth::Auto => Some(auto_pt),
                ColumnWidth::Fixed(l) => l.to_pt(em_pt, em_pt),
                ColumnWidth::Percent(p) => Some(p / 100.0 * table_pt),
                ColumnWidth::Fr(_) => None,
            })
            .collect();
        let rest = table_pt - sized.iter().flatten().sum::<f64>();
        let total_fr: f64 = widths
            .iter()
            .map(|w| if let ColumnWidth::Fr(f) = w { *f } else { 0.0 })
            .sum();
        widths
            .iter()
            .zip(sized)
            .map(|(w, pt)| match (w, pt) {
                (ColumnWidth::Fr(f), _) if rest > 0.0 => rest * f / total_fr,
                (_, Some(pt)) => pt,
                (_, None) => auto_pt,
            })
            .collect()
    }

    /// [`column_widths_pt`](Self::column_widths_pt) in a format's integer unit,
    /// `units_per_pt` to the point, rounded. Each width is at least 1, and when
    /// they would add up to more than `max_total`, they are scaled down in
    /// proportion so they don't: a format stores its widths, and the table's
    /// width, as integers of a fixed size.
    pub fn column_widths_in(
        &self,
        line_pt: f64,
        auto_pt: f64,
        em_pt: f64,
        units_per_pt: f64,
        max_total: u32,
    ) -> Vec<u32> {
        let auto = (auto_pt * units_per_pt).clamp(1.0, f64::MAX);
        let units: Vec<f64> = self
            .column_widths_pt(line_pt, auto_pt, em_pt)
            .into_iter()
            .map(|pt| {
                let u = pt * units_per_pt;
                if u.is_nan() {
                    auto
                } else {
                    u.clamp(1.0, f64::MAX)
                }
            })
            .collect();
        let total: f64 = units.iter().map(|u| u.round()).sum();
        if total <= f64::from(max_total) {
            return units.iter().map(|u| u.round() as u32).collect();
        }
        // Shares of the widest, so their sum is finite; each column gets a unit
        // and its share of the rest.
        let widest = units.iter().copied().fold(1.0, f64::max);
        let shares: Vec<f64> = units.iter().map(|u| u / widest).collect();
        let rest = f64::from(max_total.saturating_sub(units.len() as u32));
        let scale = rest / shares.iter().sum::<f64>();
        shares
            .iter()
            .map(|s| 1 + (s * scale).floor() as u32)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_positive_widths_count() {
        let of = |s: &str| ColumnWidth::of(s.parse::<Length>().ok().as_ref());
        assert_eq!(of("2cm"), ColumnWidth::Fixed(Length::Cm(2.0)));
        assert_eq!(of("1.5em"), ColumnWidth::Fixed(Length::Em(1.5)));
        assert_eq!(of("25%"), ColumnWidth::Percent(25.0));
        assert_eq!(of("2fr"), ColumnWidth::Fr(2.0));
        for s in ["auto", "0", "0px", "0%", "0fr", "-3pt", "-1fr", "bogus"] {
            assert_eq!(of(s), ColumnWidth::Auto, "{s}");
        }
        assert_eq!(ColumnWidth::of(None), ColumnWidth::Auto);
    }
}
