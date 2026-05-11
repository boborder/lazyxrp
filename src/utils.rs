pub fn xrp_to_drops(xrp: &str) -> color_eyre::Result<u64> {
    let parts: Vec<&str> = xrp.split('.').collect();
    match parts.len() {
        1 => {
            let whole: u64 = parts[0].parse()?;
            Ok(whole * 1_000_000)
        }
        2 => {
            let whole: u64 = parts[0].parse()?;
            let frac_str = format!("{:0<6}", parts[1]);
            if frac_str.len() > 6 {
                return Err(color_eyre::eyre::eyre!("XRP amount can only have up to 6 decimal places"));
            }
            let frac: u64 = frac_str.parse()?;
            Ok(whole * 1_000_000 + frac)
        }
        _ => Err(color_eyre::eyre::eyre!("Invalid XRP amount format")),
    }
}
