// SPDX-License-Identifier: MIT

use std::path::Path;

use crate::services::{ModelEncoding, ModelFormat, ModelMesh, ModelRender};

pub(super) fn read_mesh(render: ModelRender, path: &Path) -> Result<Option<ModelMesh>, String> {
    let invalid = || "The preview renderer produced invalid image data".to_owned();
    let bytes = super::read_private_output(path, 256).map_err(|_| invalid())?;
    let mesh: Option<ModelMesh> = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    let Some(mesh) = mesh else {
        return (render.format != ModelFormat::Stl)
            .then_some(None)
            .ok_or_else(invalid);
    };
    let format_matches = matches!(
        (render.format, mesh.encoding),
        (
            ModelFormat::Stl,
            ModelEncoding::BinaryStl | ModelEncoding::AsciiStl
        ) | (ModelFormat::ThreeMf, ModelEncoding::ThreeMf)
    );
    if !format_matches
        || mesh.view != render.view
        || mesh.triangles == 0
        || mesh.triangles > crate::services::model_preview::MAX_MODEL_TRIANGLES
    {
        return Err(invalid());
    }
    Ok(Some(mesh))
}

#[cfg(test)]
mod tests;
