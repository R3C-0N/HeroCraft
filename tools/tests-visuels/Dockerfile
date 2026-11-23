# Image de test de HeroCraft : compilation et lancement du client sans carte graphique
# (Vulkan logiciel lavapipe, écran virtuel Xvfb, clavier et souris pilotés par xdotool)
FROM rust:1-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
        mesa-vulkan-drivers libvulkan1 vulkan-tools libgl1-mesa-dri \
        xvfb xdotool x11-utils imagemagick \
        libx11-dev libxkbcommon-dev libxkbcommon-x11-0 libxcursor1 libxrandr2 libxi6 libwayland-client0 \
        libasound2-dev libpulse-dev pkg-config cmake \
    && rm -rf /var/lib/apt/lists/*

# Le client trouve le Vulkan logiciel ; pas de son dans le conteneur
ENV WGPU_BACKEND=vulkan \
    VK_ICD_FILENAMES=/usr/share/vulkan/icd.d/lvp_icd.x86_64.json \
    LP_NUM_THREADS=10 \
    CARGO_TARGET_DIR=/target \
    RUST_LOG=info

WORKDIR /src
