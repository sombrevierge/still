use crate::core::types::{ProcessInfo, Recommendation, SystemSnapshot};

fn gib(bytes: u64) -> f64 {
    bytes as f64 / 1_073_741_824.0
}

pub fn build(snapshot: &SystemSnapshot, processes: &[ProcessInfo]) -> Vec<Recommendation> {
    let mut result = Vec::new();
    if snapshot.memory_percent >= 82.0 || snapshot.memory_available_bytes < 1_500_000_000 {
        let mut heavy: Vec<&ProcessInfo> = processes.iter().collect();
        heavy.sort_by_key(|process| std::cmp::Reverse(process.memory_bytes));
        let names = heavy
            .iter()
            .take(3)
            .map(|process| format!("{} {:.1} GB", process.name, gib(process.memory_bytes)))
            .collect::<Vec<_>>()
            .join(", ");
        result.push(Recommendation {
            id: "memory-pressure".into(),
            kind: "pressure".into(),
            title: "Memory pressure".into(),
            detail: format!(
                "Only {:.1} GB is available. Largest users: {names}.",
                gib(snapshot.memory_available_bytes)
            ),
            action_label: Some("Inspect processes".into()),
            target_view: Some("processes".into()),
        });
    }

    if let Some(disk) = snapshot
        .disks
        .iter()
        .filter(|disk| !disk.removable)
        .min_by(|a, b| {
            a.available_bytes
                .cmp(&b.available_bytes)
                .then_with(|| a.mount.cmp(&b.mount))
        })
    {
        let free_percent = 100.0 - disk.used_percent;
        if free_percent < 12.0 {
            result.push(Recommendation {
                id: format!("disk-space-{}", disk.mount),
                kind: if free_percent < 6.0 { "pressure" } else { "attention" }.into(),
                title: format!("{} is running low", disk.mount),
                detail: format!(
                    "{:.1} GB is free ({free_percent:.0}%). Scan downloads and old bundles before Windows needs the space.",
                    gib(disk.available_bytes)
                ),
                action_label: Some("Scan storage".into()),
                target_view: Some("storage".into()),
            });
        }
    }

    let leftovers: Vec<&ProcessInfo> = processes
        .iter()
        .filter(|process| process.orphaned && process.memory_bytes > 100_000_000)
        .collect();
    if !leftovers.is_empty() {
        let memory: u64 = leftovers.iter().map(|process| process.memory_bytes).sum();
        result.push(Recommendation {
            id: "possible-leftovers".into(),
            kind: "attention".into(),
            title: "Possible leftover processes".into(),
            detail: format!(
                "{} background processes have no living parent and use {:.0} MB.",
                leftovers.len(),
                memory as f64 / 1_048_576.0
            ),
            action_label: Some("Review them".into()),
            target_view: Some("processes".into()),
        });
    }
    result
}
