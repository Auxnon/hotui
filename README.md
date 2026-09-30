# hotui

A terminal heat monitor built with [ratatui](https://ratatui.rs). Every sensor gets its own live graph: CPU (Tctl and per-CCD), NVIDIA GPU (with fan and power draw), NVMe, motherboard, NIC, Wi-Fi, or any other `hwmon` temperature.

- Each graph's Y axis covers a `min`–`max` range you set in the config.
- If a reading goes past that range, the graph rescales to fit it. Readings above `max` are drawn in **red**, a grey line marks the configured ceiling, and the panel border turns red.
- The title of each panel shows the current value, colored by how close it is to `max`, plus the lowest and highest readings for the session.

## Install

```sh
cargo install --path .
```

NVIDIA GPUs are read through NVML (`libnvidia-ml.so`, which ships with the proprietary driver). If NVML isn't there, GPU panels show an error and everything else keeps working.

## Usage

```sh
hotui                     # uses ~/.config/hotui/config.toml (created on first run)
hotui -c ./config.toml    # explicit config
hotui --print-default-config
```

| Key | Action |
| --- | --- |
| `q` / `Esc` / `Ctrl-C` | quit |
| `p` / `Space` | pause sampling |
| `r` | reload the config file |
| `+` / `-` | more / fewer columns |

## Config

```toml
[general]
refresh_ms = 1000
history_secs = 120
columns = 3

[[sensor]]
label = "CPU Tctl"
source = "hwmon"
chip = "k10temp"      # /sys/class/hwmon/hwmonN/name
sensor = "Tctl"       # match temp*_label ...
min = 30
max = 85

[[sensor]]
label = "Board VRM"
source = "hwmon"
chip = "gigabyte_wmi"
input = "temp5_input" # ... or name the file directly
min = 30
max = 80

[[sensor]]
label = "GPU"
source = "nvidia"
index = 0
min = 25
max = 83
enabled = true        # optional, default true
```

To see which sensors your machine has:

```sh
for h in /sys/class/hwmon/hwmon*; do echo "$(cat $h/name):"; ls $h | grep temp; done
```

The bundled `config.toml` is set up for a Gigabyte B550I AORUS PRO AX with a Ryzen 9 3900X, a GTX 1660, and a Gigabyte Aorus Gen4 NVMe. The `gigabyte_wmi` sensors have no labels, so the board sensor names in that file are best guesses.
