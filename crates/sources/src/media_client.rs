use anyhow::{Context, Result};

use crate::{
    fetch::{RequestProfile, SourceHttpClient},
    media::{MediaRefSpec, MediaTransformSpec},
};

#[derive(Clone)]
pub struct SourceMediaClient {
    pub(crate) http: SourceHttpClient,
}

impl SourceMediaClient {
    /// Fetches media and applies any requested media transform.
    pub async fn fetch_media(
        &self,
        media: &MediaRefSpec,
        default_profile: RequestProfile,
    ) -> Result<(Vec<u8>, String)> {
        let response = self.http.fetch_media(media, default_profile).await?;
        match media.transform {
            Some(MediaTransformSpec::ComixDescramble5x5) => Ok((
                backend_image::descramble_comix_5x5_to_png(&response.body).with_context(|| {
                    format!(
                        "failed to descramble Comix media from {} (final_url={}, content_type={}, bytes={})",
                        media.url,
                        response.final_url,
                        response.content_type,
                        response.body.len()
                    )
                })?,
                "image/png".to_string(),
            )),
            Some(MediaTransformSpec::ComixDescramble5x5Map(ref map)) => {
                let map: [usize; 25] = map
                    .as_slice()
                    .try_into()
                    .map_err(|_| anyhow::anyhow!("Comix descramble map must contain 25 tiles"))?;
                Ok((
                    backend_image::descramble_comix_5x5_with_map_to_png(&response.body, &map)
                        .with_context(|| {
                            format!(
                                "failed to descramble Comix media from {} (final_url={}, content_type={}, bytes={})",
                                media.url,
                                response.final_url,
                                response.content_type,
                                response.body.len()
                            )
                        })?,
                    "image/png".to_string(),
                ))
            }
            None => Ok((response.body, response.content_type)),
        }
    }
}
