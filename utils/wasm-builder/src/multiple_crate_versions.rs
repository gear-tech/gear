use anyhow::{Error, Result};
use cargo_metadata::{DependencyKind, Metadata, Node, Package, PackageId};
use itertools::Itertools;
use std::collections::HashSet;

const DEFAULT_DENIED_DUPLICATE_CRATES: [&str; 1] = ["gstd"];
/// Returns the list of crates name.
fn denied_duplicate_crates() -> HashSet<&'static str> {
    option_env!("GEAR_WASM_BUILDER_DENIED_DUPLICATE_CRATES").map_or_else(
        || DEFAULT_DENIED_DUPLICATE_CRATES.into(),
        |v| v.split(',').collect(),
    )
}

pub(crate) fn check(
    metadata: &Metadata,
    root_id: &PackageId,
    // denied_duplicate_crates: &HashSet<String>,
) -> Result<()> {
    let denied_duplicate_crates = denied_duplicate_crates();
    let mut packages = metadata.packages.clone();
    packages.sort_by(|a, b| a.name.cmp(&b.name));

    let mut duplicates: Vec<String> = Vec::new();

    if let Some(resolve) = &metadata.resolve {
        for (name, group) in &packages
            .iter()
            .filter(|p| denied_duplicate_crates.contains(&p.name.as_str()))
            .chunk_by(|p| &p.name)
        {
            let group: Vec<&Package> = group.collect();

            if group.len() <= 1 {
                continue;
            }

            if group
                .iter()
                .all(|p| is_normal_dep(&resolve.nodes, root_id, &p.id))
            {
                let mut versions: Vec<_> = group.into_iter().map(|p| &p.version).collect();
                versions.sort();
                let versions = versions.iter().join(", ");

                duplicates.push(format!(
                    "multiple versions for dependency `{name}`: {versions}"
                ));
            }
        }
    }

    if duplicates.is_empty() {
        Ok(())
    } else {
        Err(Error::msg(duplicates.join("\n")))
    }
}

fn is_normal_dep(nodes: &[Node], local_id: &PackageId, dep_id: &PackageId) -> bool {
    fn depends_on(node: &Node, dep_id: &PackageId) -> bool {
        node.deps.iter().any(|dep| {
            dep.pkg == *dep_id
                && dep
                    .dep_kinds
                    .iter()
                    .any(|info| matches!(info.kind, DependencyKind::Normal))
        })
    }

    nodes
        .iter()
        .filter(|node| depends_on(node, dep_id))
        .any(|node| node.id == *local_id || is_normal_dep(nodes, local_id, &node.id))
}
