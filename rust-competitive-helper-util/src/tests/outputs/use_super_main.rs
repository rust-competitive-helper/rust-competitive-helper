pub mod solution {

    use crate::algo_lib::geometry::convex_polygon_intersection::convex_polygon_intersection;

    pub fn submit() {
        convex_polygon_intersection();
    }
}
pub mod algo_lib {
    pub mod geometry {
        pub mod convex_polygon_intersection {

            use super::half_plane_intersection::half_plane_intersection;

            pub fn convex_polygon_intersection() {
                half_plane_intersection();
            }
        }
    }
}
fn main() {
    crate::solution::submit();
}
