use ash::{
    ext::debug_utils,
    khr::{surface, swapchain},
    vk, Device, Entry, Instance,
};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use simple_logger::SimpleLogger;
use std::{
    error::Error,
    ffi::{CStr, CString},
};
use vk_triangle_rs::{
    create_vulkan_instance, create_vulkan_swapchain, create_window, read_shader_from_bytes,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

const WIDTH: u32 = 800;
const HEIGHT: u32 = 600;
const APP_NAME: &str = "Triangle";


//MAIN
fn main() -> Result<(), Box<dyn Error>> {
    SimpleLogger::default().env().init()?;

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::default();
    event_loop.run_app(&mut app)?;

    Ok(())
}

//APP 
#[derive(Default)]
struct App {
    window: Option<Window>,
    triangle: Option<Triangle>,
}

impl ApplicationHandler for App {
    //создание окна и инициализиция вулкан рендера с треугольником
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = create_window(event_loop, APP_NAME, WIDTH, HEIGHT).unwrap();
        window.set_resizable(false); 

        self.triangle = Some(Triangle::new(&window).unwrap());
        self.window = Some(window);
    }

    //проверяет эвенты такие как выход и программы
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            //выход
            WindowEvent::CloseRequested => {
                event_loop.exit();
            }
            _ => (),
        }
    }

    //главный цикл рендеринга
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {

        std::thread::sleep(std::time::Duration::from_millis(16));
        let app = self.triangle.as_mut().unwrap();
        let _ = app.draw().expect("Failed to draw frame");
        //let window = self.window.as_ref().unwrap();
    }

    //конец программы
    fn exiting(&mut self, _: &ActiveEventLoop) {
        self.triangle
            .as_ref()
            .unwrap()
            .wait_for_gpu()
            .expect("Failed to wait for gpu to finish work");
    }
}
//APP END


//TRIANGLE
struct Triangle {
    //Vulkan Instance (подключение к драйверу)
    _entry: Entry,
    instance: Instance,
    debug_utils: debug_utils::Instance,
    debug_utils_messenger: vk::DebugUtilsMessengerEXT,
    //Surface (окно для рендеринга)
    surface: surface::Instance,
    surface_khr: vk::SurfaceKHR,
    //Physical Device (выбор GPU)
    physical_device: vk::PhysicalDevice,
    graphics_q_index: u32,
    present_q_index: u32,
    //Logical Device (выполнение команд)
    device: Device,
    graphics_queue: vk::Queue,
    present_queue: vk::Queue,
    //Command Management (управление командами)
    command_pool: vk::CommandPool,
    command_buffers: Vec<vk::CommandBuffer>,
    //Swapchain (буферы двойной буферизации)
    swapchain: swapchain::Device,
    swapchain_khr: vk::SwapchainKHR,
    swapchain_images: Vec<vk::Image>,
    swapchain_image_views: Vec<vk::ImageView>,
    //Rendering Pipeline (конвейер рендеринга)
    render_pass: vk::RenderPass,
    framebuffers: Vec<vk::Framebuffer>,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    //Synchronization (синхронизация GPU/CPU)
    image_available_semaphore: vk::Semaphore,
    render_finished_semaphore: vk::Semaphore,
    fence: vk::Fence,
}

impl Triangle {
    fn new(window: &Window) -> Result<Self, Box<dyn Error>> {

// Vulkan instance
        //Entry это то что подгружает vulkan
        let entry = unsafe { Entry::load()? };
        //экземпляр vulkan
        let (instance, debug_utils, debug_utils_messenger) =
            //авто заполняет VkApplicationInfo и VkInstanceCreateInfo
            create_vulkan_instance(APP_NAME, &entry, window)?;

//Surface (окно)
        let surface = surface::Instance::new(&entry, &instance);
        //Vulkan‑поверхность, привязанная к winit‑окну:
        let surface_khr = unsafe {
            ash_window::create_surface(
                &entry,
                &instance,
                window.display_handle()?.as_raw(),
                window.window_handle()?.as_raw(),
                None,
            )?
        };

//physical_device — это конкретный GPU
        //DEBUG
        let devices: Vec<vk::PhysicalDevice> = unsafe { 
            instance.enumerate_physical_devices()? 
        };
        println!("📊 Устройств Vulkan: {}", devices.len());

        for (_i, &device) in devices.iter().enumerate() {
            let props = unsafe { instance.get_physical_device_properties(device) };
            let name = unsafe { CStr::from_ptr(props.device_name.as_ptr()) }.to_str().unwrap_or("?");
            let device_type = match props.device_type {
                vk::PhysicalDeviceType::INTEGRATED_GPU => "💻 Integrated",
                vk::PhysicalDeviceType::DISCRETE_GPU => "🎮 Discrete", 
                vk::PhysicalDeviceType::VIRTUAL_GPU => "☁️ Virtual",
                vk::PhysicalDeviceType::CPU => "🖥️ CPU",
                _ => "❓ Other",
            };
            println!("GPU #: {device_type} '{name}'");
        }

        //Create Physical_device

        //graphics_q_index - для отрисовки (шейдеры, геометрия, рендеринг);
        //present_q_index - для вывода изображения на экран (подготовка swapchain‑кадров)
        let (physical_device, graphics_q_index, present_q_index) =
            create_vulkan_physical_device_and_get_graphics_and_present_qs_indices(
                &instance,
                &surface,
                surface_khr,
            )?;


//device — логическое устройство:
        //graphics_queue и present_queue — это Vulkan‑очереди
        let (device, graphics_queue, present_queue) =
            create_vulkan_device_and_graphics_and_present_qs(
                &instance,
                physical_device,
                graphics_q_index,
                present_q_index,
            )?;

// Command pool
        //команды Vulkan (рисование, копирование) записываются в командные буферы;
        //один командный буфер используется на один кадр (или несколько, если очень хочешь)
        let command_pool = {
            let command_pool_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(graphics_q_index)
                .flags(vk::CommandPoolCreateFlags::empty());
            unsafe { device.create_command_pool(&command_pool_info, None)? }
        };

// Swapchain
        //Даёт плавность 
        //Выводит изображение только если оно отрендерилось
        let (
            swapchain,
            swapchain_khr,
            swapchain_extent,//не зап. в структ.
            swapchain_format,//не зап. в структ.
            swapchain_images,
            swapchain_image_views,//image_views
        ) = create_vulkan_swapchain(
            WIDTH,
            HEIGHT,
            &instance,
            &surface,
            surface_khr,
            physical_device,
            graphics_q_index,
            present_q_index,
            &device,
        )?;

//render_pass — описывает граф рендер‑прохода
        //какие аттачменты (цвет, глубина) читаем/записываем,
        let render_pass = create_vulkan_render_pass(&device, swapchain_format)?;

//framebuffers — это обёртки вокруг swapchain_image_views:
        let framebuffers = create_vulkan_framebuffers(
            &device,
            render_pass,
            swapchain_extent,
            &swapchain_image_views,
        )?;

//pipeline и pipeline_layout — графический конвейер:
        let (pipeline, pipeline_layout) =
            create_vulkan_pipeline(&device, render_pass, swapchain_extent)?;

// Create and record command buffers (one per swapchain frame)
        let command_buffers = create_and_record_command_buffers(
            &device,
            command_pool,
            swapchain_images.len(),
            &framebuffers,
            render_pass,
            pipeline,
            swapchain_extent,
        )?;

//image_available_semaphore — событие, что следующий swapchain‑кадр готов для рисования.
        let image_available_semaphore = {
            let semaphore_info = vk::SemaphoreCreateInfo::default();
            unsafe { device.create_semaphore(&semaphore_info, None)? }
        };
//render_finished_semaphore — событие, что рендеринг кадра закончен и можно его отображать.
        let render_finished_semaphore = {
            let semaphore_info = vk::SemaphoreCreateInfo::default();
            unsafe { device.create_semaphore(&semaphore_info, None)? }
        };
//fence — барьер, который ты ждёшь (wait_for_fences) перед записью нового кадра:
        let fence = {
            let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
            unsafe { device.create_fence(&fence_info, None)? }
        };

        Ok(Self {
            //Vulkan Instance (подключение к драйверу)
            _entry: entry,
            instance,
            debug_utils,
            debug_utils_messenger,
            //Surface (окно для рендеринга)
            surface,
            surface_khr,
            //Physical Device (выбор GPU)
            physical_device,
            graphics_q_index,
            present_q_index,
            //Logical Device (выполнение команд)
            device,
            graphics_queue,
            present_queue,
            //Command Management (управление командами)
            command_pool,
            command_buffers,
            //Swapchain (буферы двойной буферизации)
            swapchain,
            swapchain_khr,
            swapchain_images,
            swapchain_image_views,
            //Rendering Pipeline (конвейер рендеринга)
            render_pass,
            framebuffers,
            pipeline,
            pipeline_layout,
            //Synchronization (синхронизация GPU/CPU)
            image_available_semaphore,
            render_finished_semaphore,
            fence,
        })
    }

    fn draw(&mut self) -> Result<bool, Box<dyn Error>> {
        let fence = self.fence;
        //Ждём завершение преведущего кадра
        unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX)? };

        //Берём кадр из swapchain
        let next_image_result = unsafe {
            self.swapchain.acquire_next_image(
                self.swapchain_khr,
                u64::MAX,
                self.image_available_semaphore,
                vk::Fence::null(),
            )
        };

        // Ok((0, false))  → "Бери кадр №0, всё ок"
        // Ok((1, true))   → "Бери кадр №1, но swapchain неоптимален" 
        // Err(OUT_OF_DATE) → "Swapchain МЁРТВ! Пересоздай!"
        // Err(ДРУГОЕ)     → "GPU сломался!"
        let image_index = match next_image_result {
            Ok((image_index, _)) => image_index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                return Ok(true);
            }
            Err(error) => panic!("Error while acquiring next image. Cause: {}", error),
        };

        //Сбрасываем
        unsafe { self.device.reset_fences(&[fence])? };

        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let wait_semaphores = [self.image_available_semaphore];
        let signal_semaphores = [self.render_finished_semaphore];

        let command_buffers = [self.command_buffers[image_index as usize]];
        let submit_info = [vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores)];

        //Отправляем на GPU
        unsafe {
            self.device.queue_submit(self.graphics_queue, &submit_info, fence)?
        };

        let swapchains = [self.swapchain_khr];
        let images_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&images_indices);

        let present_result = unsafe {
            self.swapchain
                .queue_present(self.present_queue, &present_info)
        };
        match present_result {
            Ok(is_suboptimal) if is_suboptimal => {
                return Ok(true);
            }
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                return Ok(true);
            }
            Err(error) => panic!("Failed to present queue. Cause: {}", error),
            _ => {}
        }
        Ok(false)
    }

    //ждёт завершения ВСЕХ операций на ВСЕХ очередях GPU
    pub fn wait_for_gpu(&self) -> Result<(), Box<dyn Error>> {
        unsafe { self.device.device_wait_idle()? };
        Ok(())
    }
}

//DEVICE
impl Drop for Triangle {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_fence(self.fence, None);
            self.device
                .destroy_semaphore(self.image_available_semaphore, None);
            self.device
                .destroy_semaphore(self.render_finished_semaphore, None);
            //self.cleanup_swapchain();
            self.device.destroy_command_pool(self.command_pool, None);
            self.device.destroy_device(None);
            self.surface.destroy_surface(self.surface_khr, None);
            self.debug_utils
                .destroy_debug_utils_messenger(self.debug_utils_messenger, None);
            self.instance.destroy_instance(None);
        }
    }
}

fn create_vulkan_physical_device_and_get_graphics_and_present_qs_indices(
    instance: &Instance,
    surface: &surface::Instance,
    surface_khr: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, u32, u32), Box<dyn Error>> {
    // Get the list of physical devices by prioritizing discrete then integrated gpu
    let devices = {
        let mut devices = unsafe { instance.enumerate_physical_devices()? };
        devices.sort_by_key(|device| {
            let props = unsafe { instance.get_physical_device_properties(*device) };
            match props.device_type {
                vk::PhysicalDeviceType::DISCRETE_GPU => 0,
                vk::PhysicalDeviceType::INTEGRATED_GPU => 1,
                _ => 2,
            }
        });
        devices
    };

    let mut graphics = None;
    let mut present = None;
    let device = devices
        .into_iter()
        .find(|device| {
            let device = *device;

            // Does device supports graphics and present queues
            let props = unsafe { instance.get_physical_device_queue_family_properties(device) };
            for (index, family) in props.iter().filter(|f| f.queue_count > 0).enumerate() {
                let index = index as u32;
                graphics = None;
                present = None;

                if family.queue_flags.contains(vk::QueueFlags::GRAPHICS)
                    && family.queue_flags.contains(vk::QueueFlags::COMPUTE)
                    && graphics.is_none()
                {
                    graphics = Some(index);
                }

                let present_support = unsafe {
                    surface
                        .get_physical_device_surface_support(device, index, surface_khr)
                        .expect("Failed to get device surface support")
                };
                if present_support && present.is_none() {
                    present = Some(index);
                }

                if graphics.is_some() && present.is_some() {
                    break;
                }
            }

            // Does device support desired extensions
            let extension_props = unsafe {
                instance
                    .enumerate_device_extension_properties(device)
                    .expect("Failed to get device ext properties")
            };
            let extention_support = extension_props.iter().any(|ext| {
                let name = unsafe { CStr::from_ptr(ext.extension_name.as_ptr()) };
                swapchain::NAME == name
            });

            // Does the device have available formats for the given surface
            let formats = unsafe {
                surface
                    .get_physical_device_surface_formats(device, surface_khr)
                    .expect("Failed to get physical device surface formats")
            };

            // Does the device have available present modes for the given surface
            let present_modes = unsafe {
                surface
                    .get_physical_device_surface_present_modes(device, surface_khr)
                    .expect("Failed to get physical device surface present modes")
            };

            graphics.is_some()
                && present.is_some()
                && extention_support
                && !formats.is_empty()
                && !present_modes.is_empty()
        })
        .expect("Could not find a suitable device");

    //unsafe {
        //let props = instance.get_physical_device_properties(device);
        //let device_name = CStr::from_ptr(props.device_name.as_ptr());
    //}

    Ok((device, graphics.unwrap(), present.unwrap()))
}

fn create_vulkan_device_and_graphics_and_present_qs(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    graphics_q_index: u32,
    present_q_index: u32,
) -> Result<(Device, vk::Queue, vk::Queue), Box<dyn Error>> {
    let queue_priorities = [1.0f32];
    let queue_create_infos = {
        let mut indices = vec![graphics_q_index, present_q_index];
        indices.dedup();

        indices
            .iter()
            .map(|index| {
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(*index)
                    .queue_priorities(&queue_priorities)
            })
            .collect::<Vec<_>>()
    };

    let device_extensions_ptrs = [
        swapchain::NAME.as_ptr(),
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        ash::khr::portability_subset::NAME.as_ptr(),
    ];

    let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_create_infos)
        .enabled_extension_names(&device_extensions_ptrs);

    let device = unsafe { instance.create_device(physical_device, &device_create_info, None)? };
    let graphics_queue = unsafe { device.get_device_queue(graphics_q_index, 0) };
    let present_queue = unsafe { device.get_device_queue(present_q_index, 0) };

    Ok((device, graphics_queue, present_queue))
}

fn create_vulkan_render_pass(
    device: &Device,
    format: vk::Format,
) -> Result<vk::RenderPass, Box<dyn Error>> {
    let attachment_descs = [vk::AttachmentDescription::default()
        .format(format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];

    let color_attachment_refs = [vk::AttachmentReference::default()
        .attachment(0)
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];

    let subpass_descs = [vk::SubpassDescription::default()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&color_attachment_refs)];

    let subpass_deps = [vk::SubpassDependency::default()
        .src_subpass(vk::SUBPASS_EXTERNAL)
        .dst_subpass(0)
        .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .src_access_mask(vk::AccessFlags::empty())
        .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
        .dst_access_mask(
            vk::AccessFlags::COLOR_ATTACHMENT_READ | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
        )];

    let render_pass_info = vk::RenderPassCreateInfo::default()
        .attachments(&attachment_descs)
        .subpasses(&subpass_descs)
        .dependencies(&subpass_deps);

    Ok(unsafe { device.create_render_pass(&render_pass_info, None)? })
}

fn create_vulkan_framebuffers(
    device: &Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
    image_views: &[vk::ImageView],
) -> Result<Vec<vk::Framebuffer>, Box<dyn Error>> {
    Ok(image_views
        .iter()
        .map(|view| [*view])
        .map(|attachments| {
            let framebuffer_info = vk::FramebufferCreateInfo::default()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1);
            unsafe { device.create_framebuffer(&framebuffer_info, None) }
        })
        .collect::<Result<Vec<_>, _>>()?)
}

fn create_vulkan_pipeline(
    device: &Device,
    render_pass: vk::RenderPass,
    extent: vk::Extent2D,
) -> Result<(vk::Pipeline, vk::PipelineLayout), Box<dyn Error>> {
    let layout_info = vk::PipelineLayoutCreateInfo::default();
    let pipeline_layout = unsafe { device.create_pipeline_layout(&layout_info, None)? };

    let entry_point_name = CString::new("main")?;

    let vertex_source =
        read_shader_from_bytes(&include_bytes!("../assets/shaders/shader.vert.spv")[..])?;
    let vertex_create_info = vk::ShaderModuleCreateInfo::default().code(&vertex_source);
    let vertex_module = unsafe { device.create_shader_module(&vertex_create_info, None)? };

    let fragment_source =
        read_shader_from_bytes(&include_bytes!("../assets/shaders/shader.frag.spv")[..])?;
    let fragment_create_info = vk::ShaderModuleCreateInfo::default().code(&fragment_source);
    let fragment_module = unsafe { device.create_shader_module(&fragment_create_info, None)? };

    let shader_states_infos = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module)
            .name(&entry_point_name),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_module)
            .name(&entry_point_name),
    ];

    let vertex_input_info = vk::PipelineVertexInputStateCreateInfo::default();

    let input_assembly_info = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        .primitive_restart_enable(false);

    let viewports = [vk::Viewport {
        x: 0.0,
        y: 0.0,
        width: extent.width as _,
        height: extent.height as _,
        min_depth: 0.0,
        max_depth: 1.0,
    }];
    let scissors = [vk::Rect2D {
        offset: vk::Offset2D { x: 0, y: 0 },
        extent,
    }];
    let viewport_info = vk::PipelineViewportStateCreateInfo::default()
        .viewports(&viewports)
        .scissors(&scissors);

    let rasterizer_info = vk::PipelineRasterizationStateCreateInfo::default()
        .depth_clamp_enable(false)
        .rasterizer_discard_enable(false)
        .polygon_mode(vk::PolygonMode::FILL)
        .line_width(1.0)
        .cull_mode(vk::CullModeFlags::BACK)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .depth_bias_enable(false)
        .depth_bias_constant_factor(0.0)
        .depth_bias_clamp(0.0)
        .depth_bias_slope_factor(0.0);

    let multisampling_info = vk::PipelineMultisampleStateCreateInfo::default()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::TYPE_1)
        .min_sample_shading(1.0)
        .alpha_to_coverage_enable(false)
        .alpha_to_one_enable(false);

    let color_blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
        .color_write_mask(vk::ColorComponentFlags::RGBA)
        .blend_enable(false)
        .src_color_blend_factor(vk::BlendFactor::ONE)
        .dst_color_blend_factor(vk::BlendFactor::ZERO)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD)];
    let color_blending_info = vk::PipelineColorBlendStateCreateInfo::default()
        .logic_op_enable(false)
        .logic_op(vk::LogicOp::COPY)
        .attachments(&color_blend_attachments)
        .blend_constants([0.0, 0.0, 0.0, 0.0]);

    let pipeline_info = [vk::GraphicsPipelineCreateInfo::default()
        .stages(&shader_states_infos)
        .vertex_input_state(&vertex_input_info)
        .input_assembly_state(&input_assembly_info)
        .viewport_state(&viewport_info)
        .rasterization_state(&rasterizer_info)
        .multisample_state(&multisampling_info)
        .color_blend_state(&color_blending_info)
        .layout(pipeline_layout)
        .render_pass(render_pass)
        .subpass(0)];

    let pipeline = unsafe {
        device
            .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
            .map_err(|e| e.1)?[0]
    };

    unsafe {
        device.destroy_shader_module(vertex_module, None);
        device.destroy_shader_module(fragment_module, None);
    }

    Ok((pipeline, pipeline_layout))
}

fn create_and_record_command_buffers(
    device: &Device,
    pool: vk::CommandPool,
    count: usize,
    framebuffers: &[vk::Framebuffer],
    render_pass: vk::RenderPass,
    pipeline: vk::Pipeline,
    extent: vk::Extent2D,
) -> Result<Vec<vk::CommandBuffer>, Box<dyn Error>> {
    let buffers = {
        let allocate_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(count as _);

        unsafe { device.allocate_command_buffers(&allocate_info)? }
    };

    for (index, buffer) in buffers.iter().enumerate() {
        let buffer = *buffer;

        let command_buffer_begin_info = vk::CommandBufferBeginInfo::default();
        unsafe { device.begin_command_buffer(buffer, &command_buffer_begin_info)? };

        let render_pass_begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(render_pass)
            .framebuffer(framebuffers[index])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent,
            })
            .clear_values(&[vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0], //цвет фона
                },
            }]);

        unsafe {
            device.cmd_begin_render_pass(
                buffer,
                &render_pass_begin_info,
                vk::SubpassContents::INLINE,
            )
        };

        unsafe { device.cmd_bind_pipeline(buffer, vk::PipelineBindPoint::GRAPHICS, pipeline) };

        unsafe { device.cmd_draw(buffer, 3, 1, 0, 0) };

        unsafe { device.cmd_end_render_pass(buffer) };

        unsafe { device.end_command_buffer(buffer)? };
    }

    Ok(buffers)
}
