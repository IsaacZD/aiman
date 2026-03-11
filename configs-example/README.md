# configs-example

Sample configuration seeds for local development.

Copy these into `config/` if you want default seeds for the dev shell in `flake.nix`.

```bash
cp -n configs-example/agent/engines.toml config/agent/engines.toml
cp -n configs-example/dashboard/hosts.toml config/dashboard/hosts.toml
```

If you're not using the dev shell, either set `AIMAN_ENGINES_CONFIG`/`AIMAN_HOSTS_CONFIG`
or copy into the default `configs/` paths:
```bash
cp -n configs-example/agent/engines.toml configs/engines.toml
cp -n configs-example/dashboard/hosts.toml configs/hosts.toml
```

Optional agent tuning env vars:
- `AIMAN_TOKIO_WORKERS` (cap agent runtime threads only)
- `AIMAN_HARDWARE_TTL_SECS`
- `AIMAN_HARDWARE_GPU_TIMEOUT_SECS`
- `AIMAN_HARDWARE_SKIP_GPU`
