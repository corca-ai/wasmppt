use std::collections::BTreeMap;

use super::{GenerateError, GenerateErrorCode, TableOverflowPolicy, TablePolicyData, patch::Patch};

type TableRows = Vec<BTreeMap<String, String>>;
type AppliedTablePolicy<'a> = (&'a [BTreeMap<String, String>], Option<(usize, usize)>);

pub(super) fn apply_table_policy<'a>(
    id: &str,
    rows: &'a TableRows,
    policy: Option<&TablePolicyData>,
) -> Result<AppliedTablePolicy<'a>, GenerateError> {
    let Some(policy) = policy else {
        return Ok((rows, None));
    };
    let maximum_rows = usize::try_from(policy.maximum_rows).unwrap_or(usize::MAX);
    if maximum_rows == 0 {
        return Err(GenerateError::new(
            GenerateErrorCode::InvalidTable,
            format!("table {id} maximum rows must be positive"),
        ));
    }
    if rows.len() <= maximum_rows {
        return Ok((rows, None));
    }
    match policy.overflow {
        TableOverflowPolicy::Fail => Err(GenerateError::new(
            GenerateErrorCode::InvalidTable,
            format!(
                "table {id} has {} rows, exceeding its {maximum_rows}-row limit",
                rows.len()
            ),
        )),
        TableOverflowPolicy::Clip => Ok((&rows[..maximum_rows], None)),
        TableOverflowPolicy::Shrink => Ok((rows, Some((maximum_rows, rows.len())))),
    }
}

pub(super) fn table_row_height_patch(
    template_row: &[u8],
    numerator: u64,
    denominator: u64,
) -> Result<Option<Patch>, GenerateError> {
    for marker in [b" h=\"".as_slice(), b" h='".as_slice()] {
        let Some(marker_start) = template_row
            .windows(marker.len())
            .position(|window| window == marker)
        else {
            continue;
        };
        let start = marker_start + marker.len();
        let quote = marker[marker.len() - 1];
        let Some(relative_end) = template_row[start..].iter().position(|byte| *byte == quote)
        else {
            return Err(GenerateError::new(
                GenerateErrorCode::InvalidTable,
                "table row height attribute is unterminated",
            ));
        };
        let end = start + relative_end;
        let height = std::str::from_utf8(&template_row[start..end])
            .ok()
            .and_then(|value| value.parse::<u64>().ok())
            .ok_or_else(|| {
                GenerateError::new(
                    GenerateErrorCode::InvalidTable,
                    "table row height is not an unsigned integer",
                )
            })?;
        let scaled = height
            .saturating_mul(numerator)
            .div_ceil(denominator)
            .max(1);
        return Ok(Some(Patch {
            range: start..end,
            replacement: scaled.to_string().into_bytes(),
        }));
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(count: usize) -> TableRows {
        (0..count).map(|_| BTreeMap::new()).collect()
    }

    fn policy(overflow: TableOverflowPolicy) -> TablePolicyData {
        TablePolicyData {
            maximum_rows: 2,
            overflow,
        }
    }

    #[test]
    fn overflow_policies_preserve_their_distinct_contracts() {
        let rows = rows(3);
        let error = apply_table_policy("sales", &rows, Some(&policy(TableOverflowPolicy::Fail)))
            .unwrap_err();
        assert_eq!(error.code(), GenerateErrorCode::InvalidTable);

        let (clipped, shrink) =
            apply_table_policy("sales", &rows, Some(&policy(TableOverflowPolicy::Clip))).unwrap();
        assert_eq!(clipped.len(), 2);
        assert_eq!(shrink, None);

        let (all, shrink) =
            apply_table_policy("sales", &rows, Some(&policy(TableOverflowPolicy::Shrink))).unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(shrink, Some((2, 3)));
    }

    #[test]
    fn scales_row_height_with_ceiling_rounding() {
        let patch = table_row_height_patch(b"<a:tr h=\"10\">", 2, 3)
            .unwrap()
            .unwrap();
        assert_eq!(patch.range, 9..11);
        assert_eq!(patch.replacement, b"7");
    }
}
