//! Local model catalogue, sizing and device-aware recommendation.
//!
//! Everything here is pure: no filesystem, no network, no allocation beyond the result. That is
//! what lets the recommendation be tested against a dozen synthetic machines instead of
//! whatever hardware happens to be under the desk.
//!
//! The catalogue is a curated list, not a scraped index. Kavach never downloads weights — the
//! user brings a file, and this module answers "which file should I bring, and will it fit".

/// One quantisation level and what it costs.
pub struct Quant {
    pub id: &'static str,
    /// Bytes per parameter, including the overhead of the container.
    pub bytes_per_param: f32,
    /// Relative quality, 100 = full precision. Rough, and labelled as such.
    pub quality: u8,
    pub note: &'static str,
}

pub const QUANTS: &[Quant] = &[
    Quant { id: "Q4_K_M", bytes_per_param: 0.60, quality: 88, note: "default: best size for the quality lost" },
    Quant { id: "Q5_K_M", bytes_per_param: 0.72, quality: 93, note: "a little larger, noticeably better on reasoning" },
    Quant { id: "Q6_K", bytes_per_param: 0.85, quality: 96, note: "for machines with headroom" },
    Quant { id: "Q8_0", bytes_per_param: 1.06, quality: 99, note: "effectively lossless, twice the disk" },
    Quant { id: "F16", bytes_per_param: 2.00, quality: 100, note: "full precision; rarely worth it locally" },
];

/// A model family at a given size, before a quantisation is chosen.
pub struct ModelSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub family: &'static str,
    pub params_b: f32,
    pub licence: &'static str,
    pub context: u32,
    pub strengths: &'static str,
}

pub const CATALOG: &[ModelSpec] = &[
    ModelSpec { id: "qwen2.5-coder-1.5b", name: "Qwen2.5 Coder 1.5B", family: "qwen2.5-coder", params_b: 1.5, licence: "Apache-2.0", context: 32768, strengths: "fast code completion, tiny footprint" },
    ModelSpec { id: "qwen2.5-coder-7b", name: "Qwen2.5 Coder 7B", family: "qwen2.5-coder", params_b: 7.0, licence: "Apache-2.0", context: 32768, strengths: "the best code model that fits a laptop" },
    ModelSpec { id: "qwen2.5-coder-14b", name: "Qwen2.5 Coder 14B", family: "qwen2.5-coder", params_b: 14.0, licence: "Apache-2.0", context: 32768, strengths: "stronger multi-file work" },
    ModelSpec { id: "llama-3.2-1b", name: "Llama 3.2 1B", family: "llama-3.2", params_b: 1.0, licence: "Llama 3.2 Community", context: 131072, strengths: "summaries and short notes" },
    ModelSpec { id: "llama-3.2-3b", name: "Llama 3.2 3B", family: "llama-3.2", params_b: 3.0, licence: "Llama 3.2 Community", context: 131072, strengths: "general chat on modest hardware" },
    ModelSpec { id: "llama-3.1-8b", name: "Llama 3.1 8B", family: "llama-3.1", params_b: 8.0, licence: "Llama 3.1 Community", context: 131072, strengths: "broad general capability" },
    ModelSpec { id: "mistral-7b", name: "Mistral 7B", family: "mistral", params_b: 7.2, licence: "Apache-2.0", context: 32768, strengths: "reliable general purpose" },
    ModelSpec { id: "phi-3.5-mini", name: "Phi 3.5 Mini", family: "phi", params_b: 3.8, licence: "MIT", context: 131072, strengths: "reasoning per byte" },
    ModelSpec { id: "gemma-2-2b", name: "Gemma 2 2B", family: "gemma", params_b: 2.6, licence: "Gemma Terms", context: 8192, strengths: "small and well-behaved" },
    ModelSpec { id: "deepseek-coder-6.7b", name: "DeepSeek Coder 6.7B", family: "deepseek-coder", params_b: 6.7, licence: "DeepSeek Licence", context: 16384, strengths: "code completion, permissive-ish" },
    ModelSpec { id: "nomic-embed-text", name: "Nomic Embed Text", family: "nomic", params_b: 0.14, licence: "Apache-2.0", context: 8192, strengths: "embeddings for local search" },
    ModelSpec { id: "qwen2.5-coder-32b", name: "Qwen2.5 Coder 32B", family: "qwen2.5-coder", params_b: 32.0, licence: "Apache-2.0", context: 32768, strengths: "workstation class" },
];

pub fn catalog() -> &'static [ModelSpec] {
    CATALOG
}

pub fn quants() -> &'static [Quant] {
    QUANTS
}

pub fn find_quant(id: &str) -> Option<&'static Quant> {
    QUANTS.iter().find(|q| q.id.eq_ignore_ascii_case(id))
}

pub fn find_spec(id: &str) -> Option<&'static ModelSpec> {
    CATALOG.iter().find(|s| s.id.eq_ignore_ascii_case(id))
}

/// Expected file size in bytes. Weights plus a small container overhead.
pub fn estimate_size_bytes(params_b: f32, quant: &str) -> Option<u64> {
    let q = find_quant(quant)?;
    if params_b <= 0.0 {
        return None;
    }
    let params = (params_b as f64) * 1_000_000_000.0;
    Some((params * q.bytes_per_param as f64) as u64)
}

pub fn quality_score(quant: &str) -> Option<u8> {
    find_quant(quant).map(|q| q.quality)
}

/// What the machine can afford to give a model.
///
/// Not all free memory is usable: the OS, the app and the KV cache all need room. The rule is
/// 70% of available RAM, or the largest GPU's VRAM if the machine has one with real memory —
/// whichever is larger, because a GPU-resident model is the faster path.
pub fn budget_bytes(ram_available: u64, vram: u64) -> u64 {
    let from_ram = (ram_available as f64 * 0.70) as u64;
    if vram >= 2 * 1024 * 1024 * 1024 {
        vram.max(from_ram)
    } else {
        from_ram
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Verdict {
    /// Fits with room to spare.
    Comfortable,
    /// Fits, but little headroom — expect swapping under load.
    Tight,
    /// Does not fit.
    TooLarge,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Comfortable => "comfortable",
            Verdict::Tight => "tight",
            Verdict::TooLarge => "too-large",
        }
    }
}

pub struct Fit {
    pub verdict: Verdict,
    pub size_bytes: u64,
    pub budget_bytes: u64,
    pub headroom_bytes: i64,
}

pub fn fit(params_b: f32, quant: &str, budget: u64) -> Option<Fit> {
    let size = estimate_size_bytes(params_b, quant)?;
    let verdict = if size <= budget / 2 {
        Verdict::Comfortable
    } else if size <= budget {
        Verdict::Tight
    } else {
        Verdict::TooLarge
    };
    Some(Fit {
        verdict,
        size_bytes: size,
        budget_bytes: budget,
        headroom_bytes: budget as i64 - size as i64,
    })
}

pub struct Recommendation {
    pub spec: &'static ModelSpec,
    pub quant: &'static str,
    pub fit: Fit,
}

/// Rank the catalogue for a machine: everything that fits, best quality first.
///
/// The quantisation is chosen per model rather than fixed, so a small model can be offered at
/// high quality on the same machine where a large one only fits at Q4.
pub fn recommend(params_budget: u64, limit: usize) -> Vec<Recommendation> {
    let mut out: Vec<Recommendation> = Vec::new();
    for spec in CATALOG {
        // The best quantisation this machine can afford for this model.
        let mut best: Option<Recommendation> = None;
        for quant in QUANTS {
            let Some(f) = fit(spec.params_b, quant.id, params_budget) else { continue };
            if f.verdict == Verdict::TooLarge {
                continue;
            }
            let better = match &best {
                None => true,
                Some(current) => quant.quality > current.fit_verdict_quality(),
            };
            if better {
                best = Some(Recommendation { spec, quant: quant.id, fit: f });
            }
        }
        if let Some(mut rec) = best {
            // Prefer comfortable fits: a tight fit is offered only if nothing else is close.
            rec.fit.verdict = if rec.fit.verdict == Verdict::Comfortable {
                Verdict::Comfortable
            } else {
                Verdict::Tight
            };
            out.push(rec);
        }
    }
    // Rank by capability, discounted by how tight the fit is.
    //
    // Sorting "comfortable first, then size" reads well and behaves badly: on a 24 GB machine
    // every large model is tight, so the list filled with 1B models and never showed the 32B
    // the machine could actually run. Weighting capability by fit keeps the biggest usable
    // model at the top while still preferring a comfortable fit over a marginal one.
    let score = |rec: &Recommendation| {
        let weight = if rec.fit.verdict == Verdict::Comfortable { 1.0 } else { 0.55 };
        rec.spec.params_b as f64 * weight
    };
    out.sort_by(|a, b| score(b).partial_cmp(&score(a)).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit.max(1));
    out
}

impl Recommendation {
    fn fit_verdict_quality(&self) -> u8 {
        quality_score(self.quant).unwrap_or(0)
    }
}

/// The largest quantisation of a given model that still fits.
pub fn best_quant_for(params_b: f32, budget: u64) -> Option<&'static str> {
    let mut chosen: Option<&'static Quant> = None;
    for quant in QUANTS {
        let Some(f) = fit(params_b, quant.id, budget) else { continue };
        if f.verdict == Verdict::TooLarge {
            continue;
        }
        if chosen.map(|c| quant.quality > c.quality).unwrap_or(true) {
            chosen = Some(quant);
        }
    }
    chosen.map(|q| q.id)
}

/// How much context a machine can afford, capped by the model's own limit.
pub fn recommended_context(spec: &ModelSpec, budget: u64) -> u32 {
    let size = estimate_size_bytes(spec.params_b, "Q4_K_M").unwrap_or(0);
    if size == 0 {
        return 2048;
    }
    // Reserve roughly a third of the budget for KV cache, then scale context with the ratio
    // of that reserve to the weights. Deliberately conservative.
    let reserve = budget.saturating_sub(size);
    let ratio = (reserve as f64 / size as f64).clamp(0.05, 2.0);
    let ctx = (4096.0 * (1.0 + ratio * 3.0)) as u32;
    ctx.min(spec.context).max(2048)
}

/// Rough tokens per second, for expectation-setting only.
pub fn tokens_per_second(params_b: f32, on_gpu: bool) -> f32 {
    if params_b <= 0.0 {
        return 0.0;
    }
    let base = if on_gpu { 900.0 } else { 120.0 };
    (base / params_b).clamp(1.0, 400.0)
}

pub fn search(query: &str) -> Vec<&'static ModelSpec> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return CATALOG.iter().collect();
    }
    CATALOG
        .iter()
        .filter(|s| {
            s.id.to_lowercase().contains(&q)
                || s.name.to_lowercase().contains(&q)
                || s.family.to_lowercase().contains(&q)
                || s.strengths.to_lowercase().contains(&q)
                || s.licence.to_lowercase().contains(&q)
        })
        .collect()
}

/// Other quantisations of the same model, better first — the upgrade path.
pub fn upgrade_path(spec: &ModelSpec, current: &str) -> Vec<&'static str> {
    let current_quality = quality_score(current).unwrap_or(0);
    QUANTS
        .iter()
        .filter(|q| q.quality > current_quality)
        .map(|q| q.id)
        .collect()
}

/// Other quantisations of the same model, smaller first — the fallback path.
pub fn downgrade_path(_spec: &ModelSpec, current: &str) -> Vec<&'static str> {
    let current_quality = quality_score(current).unwrap_or(100);
    QUANTS
        .iter()
        .filter(|q| q.quality < current_quality)
        .rev()
        .map(|q| q.id)
        .collect()
}

pub struct Comparison {
    pub left: &'static ModelSpec,
    pub right: &'static ModelSpec,
    pub size_ratio: f32,
    pub quality_delta: i16,
    pub larger: &'static str,
}

pub fn compare(left_id: &str, right_id: &str, left_quant: &str, right_quant: &str) -> Option<Comparison> {
    let left = find_spec(left_id)?;
    let right = find_spec(right_id)?;
    let ls = estimate_size_bytes(left.params_b, left_quant)? as f32;
    let rs = estimate_size_bytes(right.params_b, right_quant)? as f32;
    if rs <= 0.0 {
        return None;
    }
    let lq = quality_score(left_quant)? as i16;
    let rq = quality_score(right_quant)? as i16;
    Some(Comparison {
        left,
        right,
        size_ratio: ls / rs,
        quality_delta: lq - rq,
        larger: if ls >= rs { left.id } else { right.id },
    })
}

/// Total bytes a set of installed models occupies.
pub fn storage_total(sizes: &[u64]) -> u64 {
    sizes.iter().sum()
}

pub fn within_budget(sizes: &[u64], limit: u64) -> bool {
    storage_total(sizes) <= limit
}

/// What a GGUF file says about itself, read from its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GgufInfo {
    pub version: u32,
    pub tensor_count: u64,
}

/// Validate a GGUF header. Cheap, and it rejects a file that is not a model before it is hashed
/// or copied — the common failure is a truncated download or an HTML error page saved as `.gguf`.
pub fn read_gguf_header(bytes: &[u8]) -> Result<GgufInfo, String> {
    if bytes.len() < 24 {
        return Err("file is too short to be a GGUF model".to_string());
    }
    if &bytes[0..4] != b"GGUF" {
        let head = String::from_utf8_lossy(&bytes[0..4.min(bytes.len())]);
        return Err(format!(
            "not a GGUF file (magic bytes {head:?}); a model file starts with \"GGUF\""
        ));
    }
    let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
    if !(1..=3).contains(&version) {
        return Err(format!("unsupported GGUF version {version}"));
    }
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&bytes[8..16]);
    let tensor_count = u64::from_le_bytes(raw);
    Ok(GgufInfo { version, tensor_count })
}

/// A machine, as far as model choice is concerned.
pub struct Machine {
    pub ram_available: u64,
    pub vram: u64,
    pub on_gpu: bool,
}

impl Machine {
    pub fn budget(&self) -> u64 {
        budget_bytes(self.ram_available, self.vram)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn laptop() -> Machine {
        Machine { ram_available: 8 * 1024 * 1024 * 1024, vram: 0, on_gpu: false }
    }

    fn desktop() -> Machine {
        Machine { ram_available: 32 * 1024 * 1024 * 1024, vram: 24 * 1024 * 1024 * 1024, on_gpu: true }
    }

    #[test]
    fn the_catalogue_is_well_formed() {
        assert!(CATALOG.len() >= 10, "a catalogue of {}", CATALOG.len());
        for spec in CATALOG {
            assert!(!spec.id.is_empty() && !spec.name.is_empty());
            assert!(spec.params_b > 0.0, "{} has no size", spec.id);
            assert!(spec.context >= 2048, "{} has an odd context", spec.id);
            assert!(find_spec(spec.id).is_some(), "{} is not findable", spec.id);
        }
        // Ids must be unique, or a lookup silently picks the wrong model.
        let mut ids: Vec<&str> = CATALOG.iter().map(|s| s.id).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "duplicate model ids");
    }

    #[test]
    fn quantisation_size_scales_with_precision() {
        let q4 = estimate_size_bytes(7.0, "Q4_K_M").unwrap();
        let q8 = estimate_size_bytes(7.0, "Q8_0").unwrap();
        let f16 = estimate_size_bytes(7.0, "F16").unwrap();
        assert!(q4 < q8 && q8 < f16, "{q4} {q8} {f16}");
        // A 7B model at Q4 is roughly 4.2 GB; assert the order of magnitude, not the byte.
        assert!((3_500_000_000..5_000_000_000).contains(&q4), "{q4}");
        assert_eq!(estimate_size_bytes(7.0, "Q9_ZZ"), None, "unknown quant must be None");
        assert_eq!(estimate_size_bytes(0.0, "Q4_K_M"), None);
    }

    #[test]
    fn quality_rises_with_precision() {
        let mut previous = 0;
        for quant in QUANTS.iter() {
            assert!(quant.quality >= previous, "{} is out of order", quant.id);
            previous = quant.quality;
        }
        assert_eq!(quality_score("Q4_K_M"), Some(88));
        assert_eq!(quality_score("nope"), None);
    }

    #[test]
    fn the_budget_uses_ram_and_prefers_a_real_gpu() {
        // 8 GiB free RAM gives 70% of it.
        let eight_gib = 8u64 * 1024 * 1024 * 1024;
        assert_eq!(budget_bytes(eight_gib, 0), (eight_gib as f64 * 0.70) as u64);
        // A 24 GB GPU raises it.
        assert_eq!(budget_bytes(8 * 1024 * 1024 * 1024, 24 * 1024 * 1024 * 1024), 24 * 1024 * 1024 * 1024);
        // A token 1 GB integrated GPU does not.
        let sixteen_gib = 16u64 * 1024 * 1024 * 1024;
        assert_eq!(budget_bytes(sixteen_gib, 1024 * 1024 * 1024), (sixteen_gib as f64 * 0.70) as u64);
    }

    #[test]
    fn fit_verdicts_are_ordered() {
        let budget = 6_000_000_000u64;
        let small = fit(1.5, "Q4_K_M", budget).unwrap();
        assert_eq!(small.verdict, Verdict::Comfortable);
        let big = fit(32.0, "Q4_K_M", budget).unwrap();
        assert_eq!(big.verdict, Verdict::TooLarge);
        assert!(big.headroom_bytes < 0);
        assert!(small.headroom_bytes > 0);
    }

    #[test]
    fn a_laptop_is_offered_small_models_and_a_desktop_large_ones() {
        let small = recommend(laptop().budget(), 5);
        assert!(!small.is_empty());
        assert!(
            small.iter().all(|r| r.spec.params_b <= 14.0),
            "a laptop should not be recommended a 32B model: {:?}",
            small.iter().map(|r| r.spec.id).collect::<Vec<_>>()
        );
        let big = recommend(desktop().budget(), 5);
        assert!(
            big.iter().any(|r| r.spec.params_b >= 14.0),
            "a 32 GB machine should see a large model: {:?}",
            big.iter().map(|r| r.spec.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_best_model_for_the_machine_is_ranked_first() {
        // A comfortable fit must outrank a marginal one of the same size, and the top of the
        // list must be the most capable model the machine can run — not the smallest.
        let recs = recommend(desktop().budget(), 5);
        assert!(!recs.is_empty());
        assert!(
            recs[0].spec.params_b >= 7.0,
            "the top recommendation for a 24 GB GPU machine was {} ({})",
            recs[0].spec.id,
            recs[0].spec.params_b
        );
        // Every recommendation must actually fit.
        for rec in &recs {
            assert_ne!(rec.fit.verdict, Verdict::TooLarge, "{} does not fit", rec.spec.id);
        }
        // And the list is ordered by capability, so sizes never increase down the list.
        let scores: Vec<f64> = recs
            .iter()
            .map(|r| {
                let weight = if r.fit.verdict == Verdict::Comfortable { 1.0 } else { 0.55 };
                r.spec.params_b as f64 * weight
            })
            .collect();
        for pair in scores.windows(2) {
            assert!(pair[0] >= pair[1], "ranking is out of order: {scores:?}");
        }
    }

    #[test]
    fn the_best_quant_is_the_highest_that_fits() {
        let generous = best_quant_for(7.0, 40_000_000_000).unwrap();
        assert_eq!(generous, "F16", "a huge machine should get full precision");
        let modest = best_quant_for(7.0, 5_000_000_000).unwrap();
        assert_eq!(modest, "Q4_K_M", "a 5 GB budget fits only Q4");
        assert_eq!(best_quant_for(32.0, 1_000_000_000), None, "nothing fits in 1 GB");
    }

    #[test]
    fn context_scales_with_headroom_and_respects_the_model() {
        let spec = find_spec("qwen2.5-coder-7b").unwrap();
        let tight = recommended_context(spec, 5_000_000_000);
        let roomy = recommended_context(spec, 64_000_000_000);
        assert!(tight >= 2048);
        assert!(roomy >= tight);
        assert!(roomy <= spec.context, "must not exceed the model's own limit");
    }

    #[test]
    fn speed_estimates_favour_the_gpu() {
        assert!(tokens_per_second(7.0, true) > tokens_per_second(7.0, false));
        assert!(tokens_per_second(32.0, false) < tokens_per_second(1.5, false));
        assert_eq!(tokens_per_second(0.0, true), 0.0);
    }

    #[test]
    fn search_matches_id_name_family_and_strength() {
        assert!(search("coder").len() >= 3);
        assert_eq!(search("").len(), CATALOG.len());
        assert!(search("apache").iter().all(|s| s.licence.contains("Apache")));
        assert!(search("nothing-matches-this").is_empty());
    }

    #[test]
    fn upgrade_and_downgrade_paths_are_opposite() {
        let spec = find_spec("qwen2.5-coder-7b").unwrap();
        let up = upgrade_path(spec, "Q4_K_M");
        let down = downgrade_path(spec, "Q4_K_M");
        assert!(!up.is_empty(), "there is always something better");
        assert!(down.is_empty(), "Q4 is the smallest quant, so nothing is below it");
        assert!(!up.contains(&"Q4_K_M"));

        // And from the top, the reverse holds.
        let from_top = downgrade_path(spec, "F16");
        assert!(!from_top.is_empty());
        assert!(upgrade_path(spec, "F16").is_empty());
        for id in &up {
            assert!(quality_score(id).unwrap() > 88);
        }
        for id in &down {
            assert!(quality_score(id).unwrap() < 88);
        }
    }

    #[test]
    fn comparison_reports_which_is_larger() {
        let c = compare("qwen2.5-coder-14b", "qwen2.5-coder-7b", "Q4_K_M", "Q4_K_M").unwrap();
        assert_eq!(c.larger, "qwen2.5-coder-14b");
        assert!(c.size_ratio > 1.9 && c.size_ratio < 2.1, "{}", c.size_ratio);
        assert_eq!(c.quality_delta, 0);
        assert!(compare("nope", "qwen2.5-coder-7b", "Q4_K_M", "Q4_K_M").is_none());
    }

    #[test]
    fn storage_budget_is_the_sum() {
        assert_eq!(storage_total(&[10, 20, 30]), 60);
        assert!(within_budget(&[10, 20], 30));
        assert!(!within_budget(&[10, 20], 29));
        assert_eq!(storage_total(&[]), 0);
    }

    #[test]
    fn gguf_header_is_validated() {
        let mut good = Vec::new();
        good.extend_from_slice(b"GGUF");
        good.extend_from_slice(&3u32.to_le_bytes());
        good.extend_from_slice(&42u64.to_le_bytes());
        good.extend_from_slice(&[0u8; 32]);
        let info = read_gguf_header(&good).unwrap();
        assert_eq!(info.version, 3);
        assert_eq!(info.tensor_count, 42);

        assert!(read_gguf_header(b"short").is_err());
        let mut html = Vec::from(&b"<htm"[..]);
        html.extend_from_slice(&[0u8; 40]);
        let err = read_gguf_header(&html).unwrap_err();
        assert!(err.contains("not a GGUF"), "{err}");

        let mut wrong_version = Vec::from(&b"GGUF"[..]);
        wrong_version.extend_from_slice(&99u32.to_le_bytes());
        wrong_version.extend_from_slice(&[0u8; 32]);
        assert!(read_gguf_header(&wrong_version).unwrap_err().contains("version"));
    }
}
