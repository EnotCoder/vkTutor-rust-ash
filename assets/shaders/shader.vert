#version 450
#extension GL_ARB_separate_shader_objects : enable

const vec2 POSITIONS[6] = vec2[](
    vec2(-0.5, -0.5),
    vec2( 0.5,  0.5),
    vec2( 0.5, -0.5),

    vec2(-0.5, -0.5),
    vec2(-0.5,  0.5),
    vec2( 0.5,  0.5)
);

layout(location = 0) out vec3 oColor;

void main() {
    vec2 position = POSITIONS[gl_VertexIndex];
    oColor = vec3(1.0, 0.0, 0.0);

    gl_Position = vec4(position.x, position.y, 0.0, 1.0);
}
