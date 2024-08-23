use cosmic_text::{
    CacheKey, FontSystem, LayoutGlyph, LayoutRun, PhysicalGlyph, Placement, SwashCache,
    SwashContent, SwashImage,
};
use egui::{
    pos2, vec2, Color32, ColorImage, NumExt, Painter, Rect, TextureHandle, TextureId,
    TextureOptions, Vec2,
};
use etagere::{size2, Allocation, BucketedAtlasAllocator, Size};
use imgref::{Img, ImgRefMut};
use lru::LruCache;
use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::hash::BuildHasher;

#[derive(Clone)]
struct GlyphState {
    allocation: Allocation,
    placement: Placement,
    colorable: bool,
}

fn write_glyph_image(image: SwashImage, default_color: Color32, mut sub_image: ImgRefMut<Color32>) {
    debug_assert!(
        sub_image.width() == image.placement.width as usize
            && sub_image.height() == image.placement.height as usize
    );
    match image.content {
        SwashContent::Mask => {
            image
                .data
                .into_iter()
                .zip(sub_image.pixels_mut())
                .for_each(|(a, slot)| {
                    *slot = Color32::from_rgba_unmultiplied(
                        default_color.r(),
                        default_color.g(),
                        default_color.b(),
                        a,
                    );
                });
        }
        SwashContent::Color => {
            image
                .data
                .chunks_exact(4)
                .zip(sub_image.pixels_mut())
                .for_each(|(pixel, slot)| {
                    let [r, g, b, a] = pixel.try_into().unwrap();
                    *slot = Color32::from_rgba_premultiplied(r, g, b, a)
                });
        }
        SwashContent::SubpixelMask => unimplemented!(),
    };
}

pub struct GlyphImage {
    atlas_texture_id: TextureId,
    uv_rect: Rect,
    default_color: Color32,
    colorable: bool,
    top: i32,
    left: i32,
    width: f32,
    height: f32,
}

impl GlyphImage {
    fn new(
        atlas_texture: &TextureHandle,
        etagere::Rectangle { min, .. }: etagere::Rectangle,
        placement: Placement,
        default_color: Color32,
        colorable: bool,
    ) -> Self {
        let atlas_texture_id = atlas_texture.id();
        let [atlas_width, atlas_height] = atlas_texture.size().map(|x| x as f32);
        let [glyph_width, glyph_height] = [placement.width, placement.height].map(|x| x as f32);
        let uv_rect = Rect::from_min_size(
            pos2(min.x as f32 / atlas_width, min.y as f32 / atlas_height),
            vec2(glyph_width / atlas_width, glyph_height / atlas_height),
        );
        Self {
            atlas_texture_id,
            uv_rect,
            default_color,
            colorable,
            top: placement.top,
            left: placement.left,
            width: glyph_width,
            height: glyph_height,
        }
    }

    pub fn paint(
        self,
        layout_glyph: &LayoutGlyph,
        physical_glyph: PhysicalGlyph,
        run: &LayoutRun,
        painter: &mut Painter,
    ) {
        let x = physical_glyph.x + self.left;
        let y = run.line_y as i32 + physical_glyph.y - self.top;

        let color_override = layout_glyph
            .color_opt
            // Is this right?
            .map(|x| Color32::from_rgba_premultiplied(x.r(), x.g(), x.b(), x.a()));

        // Note: this isn't exactly working
        let tint = match self.colorable {
            true => color_override.unwrap_or(self.default_color),
            false => Color32::WHITE,
        };

        let pixels_per_point = painter.ctx().pixels_per_point();

        painter.image(
            self.atlas_texture_id,
            Rect::from_min_size(pos2(x as f32, y as f32), vec2(self.width, self.height))
                / pixels_per_point, // Convert from physical -> logical
            self.uv_rect,
            tint,
        );
    }
}

/// **The atlas is in physical pixels**
pub struct TextureAtlas<S: BuildHasher + Default = RandomState> {
    packer: BucketedAtlasAllocator,
    cache: LruCache<CacheKey, Option<GlyphState>, S>,
    in_use: HashSet<CacheKey, S>,
    atlas_side: usize,
    max_texture_side: usize,
    texture: TextureHandle,
    ctx: egui::Context,
    default_color: Color32,
}

impl<S: BuildHasher + Default> TextureAtlas<S> {
    const ATLAS_TEXTURE_NAME: &'static str = "egui cosmic text atlas";

    pub fn new(ctx: egui::Context, default_color: Color32) -> Self {
        let atlas_side = 256_usize;
        let packer = BucketedAtlasAllocator::new(Size::splat(atlas_side as i32));
        let texture = ctx.load_texture(
            Self::ATLAS_TEXTURE_NAME,
            ColorImage::new([atlas_side, atlas_side], Color32::TRANSPARENT),
            TextureOptions::NEAREST,
        );
        Self {
            packer,
            cache: LruCache::unbounded_with_hasher(S::default()),
            in_use: HashSet::with_hasher(S::default()),
            atlas_side,
            max_texture_side: ctx.input(|i| i.max_texture_side),
            texture,
            ctx,
            default_color,
        }
    }

    fn grow(&mut self, font_system: &mut FontSystem, swash_cache: &mut SwashCache) {
        assert!(self.atlas_side < self.max_texture_side);

        let new_side_size = (self.atlas_side * 2).at_most(self.max_texture_side);
        self.atlas_side = new_side_size;

        self.packer.grow(Size::splat(new_side_size as i32));

        let mut new_atlas_image = Img::new(
            vec![Color32::TRANSPARENT; new_side_size * new_side_size],
            new_side_size,
            new_side_size,
        );

        self.cache
            .iter()
            .filter_map(|(cache_key, state)| state.as_ref().map(|state| (cache_key, state.clone())))
            .for_each(|(&cache_key, cached_glyph_state)| {
                let image = swash_cache
                    .get_image_uncached(font_system, cache_key)
                    .unwrap();
                let rect = cached_glyph_state.allocation.rectangle;
                let region = new_atlas_image.sub_image_mut(
                    rect.min.x as usize,
                    rect.min.y as usize,
                    image.placement.width as usize,
                    image.placement.height as usize,
                );
                write_glyph_image(image, self.default_color, region);
            });

        self.texture = self.ctx.load_texture(
            Self::ATLAS_TEXTURE_NAME,
            ColorImage {
                size: [new_atlas_image.width(), new_atlas_image.height()],
                pixels: new_atlas_image.into_buf(),
            },
            TextureOptions::NEAREST,
        );
    }

    fn alloc_packer(&mut self, width: u32, height: u32) -> Option<Allocation> {
        let size = size2(width as i32, height as i32);
        // Will keep freeing up unused glyphs until it can be allocated or
        // until we know that we truly ran out of space and need to grow the atlas
        loop {
            let allocation = self.packer.allocate(size);
            if allocation.is_some() {
                return allocation;
            }
            let unused_glyph = loop {
                let (key, _) = self.cache.peek_lru()?;

                // Check if this is currently being used this frame
                if self.in_use.contains(key) {
                    // We have to grow
                    return None;
                }

                let (_, value) = self.cache.pop_lru()?;

                match value {
                    // Glyph isn't sized
                    None => continue,
                    Some(x) => break x,
                }
            };
            self.packer.deallocate(unused_glyph.allocation.id);
        }
    }

    fn promote(&mut self, cache_key: CacheKey) {
        self.cache.promote(&cache_key);
        self.in_use.insert(cache_key);
    }

    fn put(&mut self, cache_key: CacheKey, value: Option<GlyphState>) {
        self.cache.put(cache_key, value);
        self.in_use.insert(cache_key);
    }

    /// Allocates in the texture atlas and returns a glyph image if applicable.
    /// Errors currently panic.
    pub fn alloc(
        &mut self,
        cache_key: CacheKey,
        font_system: &mut FontSystem,
        swash_cache: &mut SwashCache,
    ) -> Option<GlyphImage> {
        let glyph_state = (match self.cache.get(&cache_key) {
            None => {
                let image = swash_cache.get_image_uncached(font_system, cache_key)?;
                if image.placement.width == 0 || image.placement.height == 0 {
                    self.put(cache_key, None);
                    return None;
                }
                loop {
                    let alloc = self.alloc_packer(image.placement.width, image.placement.height);
                    match alloc {
                        None => self.grow(font_system, swash_cache),
                        Some(x) => {
                            let glyph_state = Some(GlyphState {
                                allocation: x,
                                placement: image.placement,
                                // ?
                                colorable: matches!(image.content, SwashContent::Mask),
                            });

                            self.put(cache_key, glyph_state.clone());

                            let [width, height] = [
                                image.placement.width as usize,
                                image.placement.height as usize,
                            ];
                            let mut pixels = vec![Color32::TRANSPARENT; width * height];
                            write_glyph_image(
                                image,
                                self.default_color,
                                Img::new(&mut pixels, width, height),
                            );

                            self.texture.set_partial(
                                x.rectangle.min.to_array().map(|x| x as usize),
                                ColorImage {
                                    size: [width, height],
                                    pixels,
                                },
                                TextureOptions::NEAREST,
                            );

                            break glyph_state;
                        }
                    }
                }
            }
            Some(x) => {
                let state = x.clone();
                self.promote(cache_key);
                state
            }
        })?;

        Some(GlyphImage::new(
            &self.texture,
            glyph_state.allocation.rectangle,
            glyph_state.placement,
            self.default_color,
            glyph_state.colorable,
        ))
    }

    pub fn atlas_texture(&self) -> TextureId {
        self.texture.id()
    }

    pub fn atlas_texture_size(&self) -> Vec2 {
        self.texture.size_vec2()
    }

    pub fn update_max_texture_side(&mut self) {
        self.max_texture_side = self.ctx.input(|i| i.max_texture_side)
    }

    pub fn trim(&mut self) {
        self.in_use.clear()
    }
}

#[cfg(test)]
mod tests {
    use crate::atlas::{GlyphImage, GlyphState};
    use cosmic_text::{CacheKey, Placement};
    use etagere::Allocation;

    #[test]
    fn test() {
        dbg!(std::mem::size_of::<Option<GlyphState>>());
        dbg!(std::mem::size_of::<Option<(Allocation, Placement)>>());
        dbg!(std::mem::size_of::<CacheKey>());
        dbg!(std::mem::size_of::<GlyphImage>());
    }
}
