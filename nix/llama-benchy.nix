{
  lib,
  python3Packages,
  fetchFromGitHub,
}:
python3Packages.buildPythonPackage {
  pname = "llama-benchy";
  version = "0-unstable-2026-03-10";

  src = fetchFromGitHub {
    owner = "eugr";
    repo = "llama-benchy";
    rev = "d03aaaf8998a70ab41d65e16cfe79549b13b6ce7";
    hash = "sha256-jXoB9RIgJpV3SUl5MzwS66AWIHItVsaV/H18QUrUors=";
  };

  pyproject = true;

  build-system = with python3Packages; [
    hatchling
    hatch-vcs
  ];

  dependencies = with python3Packages; [
    openai
    transformers
    torch
    tabulate
    numpy
    requests
    aiohttp
    pydantic
  ];

  nativeBuildInputs = [ python3Packages.pythonRelaxDepsHook ];
  pythonRelaxDeps = [ "pydantic" "transformers" ];
  pythonRemoveDeps = [ "asyncio" ];

  env.SETUPTOOLS_SCM_PRETEND_VERSION = "0.0.1";

  doCheck = false;

  meta = with lib; {
    description = "llama-bench style benchmarking tool for OpenAI-compatible LLM endpoints";
    homepage = "https://github.com/eugr/llama-benchy";
    license = licenses.mit;
    platforms = platforms.linux;
  };
}
