export type EnvVar = {
  key: string;
  value: string;
};

export type ContainerImageBuildForm = {
  enabled: boolean;
  dockerfile_content: string;
  pull: boolean;
  no_cache: boolean;
  build_args: EnvVar[];
};

export type ContainerImageForm = {
  id: string;
  name: string;
  image: string;
  ports: string[];
  volumes: string[];
  env: EnvVar[];
  run_args: string[];
  gpus: string;
  user: string;
  command: string;
  args: string[];
  pull: boolean;
  remove: boolean;
  build: ContainerImageBuildForm;
};

export type ContainerEngineForm = {
  image_id: string;
  container_name: string;
  extra_ports: string[];
  extra_volumes: string[];
  extra_env: EnvVar[];
  extra_run_args: string[];
  gpus: string;
  user: string;
  command: string;
  args: string[];
  pull_mode: "inherit" | "true" | "false";
  remove_mode: "inherit" | "true" | "false";
};

export function createContainerImageForm(): ContainerImageForm {
  return {
    id: "",
    name: "",
    image: "",
    ports: [],
    volumes: [],
    env: [],
    run_args: [],
    gpus: "",
    user: "",
    command: "",
    args: [],
    pull: false,
    remove: true,
    build: {
      enabled: false,
      dockerfile_content: "",
      pull: false,
      no_cache: false,
      build_args: []
    }
  };
}

export function createContainerEngineForm(): ContainerEngineForm {
  return {
    image_id: "",
    container_name: "",
    extra_ports: [],
    extra_volumes: [],
    extra_env: [],
    extra_run_args: [],
    gpus: "",
    user: "",
    command: "",
    args: [],
    pull_mode: "inherit",
    remove_mode: "inherit"
  };
}
