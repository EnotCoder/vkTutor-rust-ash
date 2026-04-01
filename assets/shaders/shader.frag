#version 450
layout(push_constant) uniform PushConstants {
    vec4 color;
} push;

layout(location = 0) out vec4 outColor;
void main() {
    outColor = push.color;  // ← Получаем цвет из Rust!
}