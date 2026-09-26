use erf_incremental_baseline::{
    default_budget_policy, synthetic_fixture_matrix, BenchmarkBand, FixtureScale, FixtureTimingSample,
    PerformanceReport, Stage,
};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

fn parse_scale(value: &str) -> Result<FixtureScale, Box<dyn std::error::Error>> {
    match value {
        "SMALL" => Ok(FixtureScale::Small),
        "MEDIUM" => Ok(FixtureScale::Medium),
        "LARGE" => Ok(FixtureScale::Large),
        _ => Err(format!("unknown fixture scale: {value}").into()),
    }
}

fn parse_band(value: &str) -> Result<BenchmarkBand, Box<dyn std::error::Error>> {
    match value {
        "FULL" => Ok(BenchmarkBand::Full),
        "INCREMENTAL_COLD" => Ok(BenchmarkBand::IncrementalCold),
        "INCREMENTAL_WARM" => Ok(BenchmarkBand::IncrementalWarm),
        _ => Err(format!("unknown benchmark band: {value}").into()),
    }
}

fn attach_fixture_timings(
    cases: &mut [erf_incremental_baseline::BenchmarkCase],
    raw: &str,
) -> Result<Vec<FixtureTimingSample>, Box<dyn std::error::Error>> {
    let mut next_repetition = BTreeMap::<(FixtureScale, BenchmarkBand), usize>::new();
    let mut samples = Vec::<FixtureTimingSample>::new();
    for line in raw.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with('#')) {
        let mut fields = line.split('\t');
        let scale = parse_scale(fields.next().ok_or("timing row is missing scale")?)?;
        let band = parse_band(fields.next().ok_or("timing row is missing band")?)?;
        let repetition: usize = fields.next().ok_or("timing row is missing repetition")?.parse()?;
        let elapsed_nanos: u64 = fields.next().ok_or("timing row is missing elapsed nanos")?.parse()?;
        if repetition >= 3 {
            return Err(format!("timing row has invalid repetition: {repetition}").into());
        }
        if fields.next().is_some() {
            return Err("timing row has unexpected fields".into());
        }
        let expected_repetition = next_repetition.entry((scale, band)).or_insert(0);
        if *expected_repetition != repetition {
            return Err(format!("timing rows must contain repetitions 0,1,2 in order for {scale:?}/{band:?}").into());
        }
        let mut matching_position = 0;
        let mut matched = false;
        for case in cases.iter_mut().filter(|case| case.scale == scale && case.band() == band) {
            if matching_position == repetition {
                case.replace_stage_elapsed(Stage::Compile, elapsed_nanos)?;
                matched = true;
            }
            matching_position += 1;
        }
        if !matched {
            return Err(format!("timing row has no matching benchmark slice: {scale:?}/{band:?}").into());
        }
        samples.push(FixtureTimingSample::new(scale, band, repetition as u8, elapsed_nanos)?);
        *expected_repetition += 1;
    }
    if samples.is_empty() {
        return Err("fixture timing input is empty".into());
    }
    Ok(samples)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = env::args().nth(1).map(PathBuf::from).ok_or("usage: render_baseline <output.md>")?;
    let timing_input = env::args().nth(2).map(PathBuf::from);
    let mut cases = synthetic_fixture_matrix()?;
    let timing_provenance = if let Some(path) = timing_input {
        let raw = fs::read_to_string(path)?;
        let samples = attach_fixture_timings(&mut cases, &raw)?;
        Some(samples)
    } else {
        None
    };
    let policy = default_budget_policy();
    let mut report = PerformanceReport::evaluate(&cases, &policy)?;
    if let Some(samples) = timing_provenance {
        report.attach_fixture_timing(&samples)?;
    }
    if report.repository_count < 3 {
        return Err("synthetic baseline must contain at least three repository identities".into());
    }
    fs::write(&output, report.to_markdown(&policy))?;
    println!("incremental-baseline={} status={}", output.display(), report.status.code());
    Ok(())
}
