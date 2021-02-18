

#[derive(Debug, Clone, PartialEq)]
pub struct CurvHeader {
    pub curv_magic: [u8; 3],
    pub num_vertices: i32,
    pub num_faces: i32,
    pub num_values_per_vertex: i32,
}

