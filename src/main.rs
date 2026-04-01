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
    create_vulkan_instance,
    create_vulkan_swapchain,
    create_window,
    read_shader_from_bytes,
    create_vulkan_physical_device_and_get_graphics_and_present_qs_indices,
    create_vulkan_device_and_graphics_and_present_qs,
    create_vulkan_render_pass,
    create_vulkan_framebuffers,
    create_vulkan_pipeline,
    create_and_record_command_buffers,
};
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowId},
};

use vk_triangle_rs::PushConstants;

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

        //ImageView  = "Картина" (800x600 пикселей)
        //Framebuffer = "Рамка + инструкция как вешать"
        //RenderPass = "Инструкция: залей чёрным → рисуй треугольник"

//pipeline и pipeline_layout — графический конвейер:
        let (pipeline, pipeline_layout) =
            create_vulkan_pipeline(&device, render_pass, swapchain_extent)?;

//COMMAND_BUFFER → cmd_draw() ← Рисует В Framebuffer0 → Image0 → ЭКРАН!
        let command_buffers = create_and_record_command_buffers(
            &device,
            command_pool,
            swapchain_images.len(),
            &framebuffers,
            render_pass,
            pipeline,
            swapchain_extent,
            pipeline_layout,
        )?;

//EVENTS
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
        unsafe { self.device.wait_for_fences(&[fence], true, u64::MAX)? };

        let next_image_result = unsafe {
            self.swapchain.acquire_next_image(
                self.swapchain_khr,
                u64::MAX,
                self.image_available_semaphore,
                vk::Fence::null(),
            )
        };

        let image_index = match next_image_result {
            Ok((image_index, _)) => image_index,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
            Err(error) => panic!("Error while acquiring next image. Cause: {}", error),
        };

        //передаём цвет
        let color = PushConstants { color: [1.0, 1.0, 1.0, 1.0] };
        let color_bytes = unsafe {
            std::slice::from_raw_parts(
                &color as *const _ as *const u8,
                std::mem::size_of::<PushConstants>(),
            )
        };

        //делаем тоже что и в create_and_record_command_buffers
        unsafe {
            let cb = self.command_buffers[image_index as usize];
            self.device.begin_command_buffer(cb, &vk::CommandBufferBeginInfo::default())?;

            let render_pass_begin_info = vk::RenderPassBeginInfo::default()
                .render_pass(self.render_pass)
                .framebuffer(self.framebuffers[image_index as usize])
                .render_area(vk::Rect2D {
                    offset: vk::Offset2D { x: 0, y: 0 },
                    extent: vk::Extent2D { width: WIDTH, height: HEIGHT },
                })
                .clear_values(&[vk::ClearValue {
                    color: vk::ClearColorValue { float32: [0.1, 0.1, 0.2, 1.0] },
                }]);

            self.device.cmd_begin_render_pass(cb, &render_pass_begin_info, vk::SubpassContents::INLINE);
            self.device.cmd_bind_pipeline(cb, vk::PipelineBindPoint::GRAPHICS, self.pipeline);
            
            self.device.cmd_push_constants(
                cb,
                self.pipeline_layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                color_bytes,
            );
            
            self.device.cmd_draw(cb, 6, 1, 0, 0);
            self.device.cmd_end_render_pass(cb);
            self.device.end_command_buffer(cb)?;
        }

        // СБРАСЫВАЕМ FENCE
        unsafe { self.device.reset_fences(&[fence])?; }

        // 🔥 ВОССТАНАВЛИВАЕМ ПЕРЕМЕННЫЕ ДЛЯ SUBMIT
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        let wait_semaphores = [self.image_available_semaphore];
        let signal_semaphores = [self.render_finished_semaphore];
        let command_buffers = [self.command_buffers[image_index as usize]];

        let submit_info = [vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers)
            .signal_semaphores(&signal_semaphores)];

        unsafe { self.device.queue_submit(self.graphics_queue, &submit_info, fence)? };

        let swapchains = [self.swapchain_khr];
        let images_indices = [image_index];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&images_indices);

        let present_result = unsafe {
            self.swapchain.queue_present(self.present_queue, &present_info)
        };
        
        match present_result {
            Ok(is_suboptimal) if is_suboptimal => return Ok(true),
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
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
