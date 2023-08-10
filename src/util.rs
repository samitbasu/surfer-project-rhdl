/// This function takes a number and converts it's digits into the range
/// a-p. This is nice because it makes for some easily typed ids.
/// The function first formats the number as a hex digit and then performs
/// the mapping.
pub fn uint_idx_to_alpha_idx(idx: usize, nsignals: usize) -> String {
    // this calculates how many hex digits we need to represent nsignals
    // unwrap because the result should always fit into usize and because
    // we are not going to display millions of character ids.
    let width = usize::try_from(nsignals.ilog(16)).unwrap() + 1;
    format!("{:0width$x}", idx)
        .chars()
        .map(|c| match c {
            '0' => 'a',
            '1' => 'b',
            '2' => 'c',
            '3' => 'd',
            '4' => 'e',
            '5' => 'f',
            '6' => 'g',
            '7' => 'h',
            '8' => 'i',
            '9' => 'j',
            'a' => 'k',
            'b' => 'l',
            'c' => 'm',
            'd' => 'n',
            'e' => 'o',
            'f' => 'p',
            _ => '?',
        })
        .collect()
}

/// This is the reverse function to uint_idx_to_alpha_idx.
pub fn alpha_idx_to_uint_idx(idx: String) -> Option<usize> {
    let mapped = idx
        .chars()
        .map(|c| match c {
            'a' => '0',
            'b' => '1',
            'c' => '2',
            'd' => '3',
            'e' => '4',
            'f' => '5',
            'g' => '6',
            'h' => '7',
            'i' => '8',
            'j' => '9',
            'k' => 'a',
            'l' => 'b',
            'm' => 'c',
            'n' => 'd',
            'o' => 'e',
            'p' => 'f',
            _ => '?',
        })
        .collect::<String>();
    usize::from_str_radix(&mapped, 16).ok()
}
