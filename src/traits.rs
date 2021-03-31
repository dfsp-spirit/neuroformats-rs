
pub trait VertexColor {
    fn vertex_color_rgb(&self) -> Vec<u8>;
    fn vertex_color_rgba(&self) -> Vec<u8>;
}