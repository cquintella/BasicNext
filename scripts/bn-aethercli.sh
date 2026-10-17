#!/usr/bin/env bash
# scripts/bn-aethercli.sh
# Wrapper de execucao do AetherCLI configurado com o ambiente do Basic Next
set -eu

# Resolve diretorios absolutos
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
CONFIG_FILE="${SCRIPT_DIR}/aethercli.json"

# Localiza o binario do aethercli:
# 1. /usr/local/bin/aethercli (se instalado)
# 2. PATH do sistema
# 3. Build local no diretorio src
AETHER_BIN=""
if [[ -x "/usr/local/bin/aethercli" ]]; then
    AETHER_BIN="/usr/local/bin/aethercli"
elif command -v aethercli >/dev/null 2>&1; then
    AETHER_BIN="$(command -v aethercli)"
elif [[ -x "/Users/caq/src/aethercli-main/build/aethercli" ]]; then
    AETHER_BIN="/Users/caq/src/aethercli-main/build/aethercli"
else
    echo "%% Erro: executavel 'aethercli' nao encontrado em /usr/local/bin, PATH ou build local." >&2
    exit 1
fi

# Assegura que a execucao ocorra a partir da raiz do repositorio Basic Next
cd "${REPO_ROOT}"

# Repassa a configuracao e todos os argumentos adicionais
# Permite:
#   scripts/bn-aethercli.sh               (shell interativo)
#   scripts/bn-aethercli.sh -p "board"    (modo headless)
#   scripts/bn-aethercli.sh --mcp         (servidor MCP)
exec "${AETHER_BIN}" -C "${CONFIG_FILE}" "$@"
