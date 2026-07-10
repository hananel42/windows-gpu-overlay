use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING, D3D11CreateDevice, ID3D11Device,
    ID3D11DeviceContext, ID3D11Resource, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};
use windows::Win32::Graphics::Dxgi::{
    CreateDXGIFactory1, DXGI_ERROR_ACCESS_LOST, IDXGIAdapter1, IDXGIFactory1, IDXGIOutput1,
    IDXGIOutputDuplication,
};
use windows::core::Interface;

pub struct DxgiScreenCapture {
    _d3d11_device: ID3D11Device,
    d3d11_context: ID3D11DeviceContext,
    duplication: IDXGIOutputDuplication,
    staging_texture: ID3D11Texture2D,
    wgpu_texture: wgpu::Texture,
    wgpu_view: wgpu::TextureView,
    width: u32,
    height: u32,
}

impl DxgiScreenCapture {
    pub fn new(wgpu_device: &wgpu::Device) -> Option<Self> {
        unsafe {
            let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
            let adapter: IDXGIAdapter1 = factory.EnumAdapters1(0).ok()?;
            let output = adapter.EnumOutputs(0).ok()?;
            let output1: IDXGIOutput1 = output.cast().ok()?;

            let mut d3d11_device = None;
            let mut d3d11_context = None;

            D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                windows::Win32::Graphics::Direct3D11::D3D11_SDK_VERSION,
                Some(&mut d3d11_device),
                None,
                Some(&mut d3d11_context),
            )
            .ok()?;

            let d3d11_device = d3d11_device?;
            let d3d11_context = d3d11_context?;

            let duplication = output1.DuplicateOutput(&d3d11_device).ok()?;
            let desc = duplication.GetDesc();
            let width = desc.ModeDesc.Width;
            let height = desc.ModeDesc.Height;

            let staging_desc = D3D11_TEXTURE2D_DESC {
                Width: width,
                Height: height,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };

            let mut staging_texture = None;
            d3d11_device
                .CreateTexture2D(&staging_desc, None, Some(&mut staging_texture))
                .ok()?;
            let staging_texture = staging_texture?;

            let wgpu_texture = wgpu_device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Dxgi Screen Record Texture"),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Bgra8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });

            let wgpu_view = wgpu_texture.create_view(&wgpu::TextureViewDescriptor::default());

            Some(Self {
                _d3d11_device: d3d11_device,
                d3d11_context,
                duplication,
                staging_texture,
                wgpu_texture,
                wgpu_view,
                width,
                height,
            })
        }
    }

    pub fn update_and_get_view(&mut self, wgpu_queue: &wgpu::Queue) -> &wgpu::TextureView {
        unsafe {
            let mut frame_resource = None;
            let mut frame_info = Default::default();

            let res = self
                .duplication
                .AcquireNextFrame(5, &mut frame_info, &mut frame_resource);

            if res.is_ok() {
                if let Some(resource) = frame_resource {
                    let next_surface: ID3D11Texture2D = resource.cast().unwrap();

                    // המרה מפורשת ל-ID3D11Resource כדי לספק את ה-Trait Bound
                    let dst_resource: ID3D11Resource = self.staging_texture.cast().unwrap();
                    let src_resource: ID3D11Resource = next_surface.cast().unwrap();

                    self.d3d11_context
                        .CopyResource(&dst_resource, &src_resource);

                    // יצירת מבנה ה-Mapped הריק מראש ומעבר שלו כארגומנט חמישי
                    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
                    if self
                        .d3d11_context
                        .Map(&dst_resource, 0, D3D11_MAP_READ, 0, Some(&mut mapped))
                        .is_ok()
                    {
                        let raw_bytes = std::slice::from_raw_parts(
                            mapped.pData as *const u8,
                            (self.width * self.height * 4) as usize,
                        );

                        // שימוש בשמות המדויקים עבור wgpu 30
                        // שימוש בנתיב המלא והעדכני ביותר של גרסה 30:
                        wgpu_queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                texture: &self.wgpu_texture,
                                mip_level: 0,
                                origin: wgpu::Origin3d::ZERO,
                                aspect: wgpu::TextureAspect::All,
                            },
                            raw_bytes,
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(self.width * 4),
                                rows_per_image: Some(self.height),
                            },
                            wgpu::Extent3d {
                                width: self.width,
                                height: self.height,
                                depth_or_array_layers: 1,
                            },
                        );

                        self.d3d11_context.Unmap(&dst_resource, 0);
                    }
                }
                let _ = self.duplication.ReleaseFrame();
            } else if res.err().map(|e| e.code()) == Some(DXGI_ERROR_ACCESS_LOST) {
                println!("[DxgiCapture] Access lost. Desktop switched or resolution altered.");
            }
        }

        &self.wgpu_view
    }

    pub fn texture_view(&self) -> &wgpu::TextureView {
        &self.wgpu_view
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}
