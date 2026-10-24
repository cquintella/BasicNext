#!/usr/bin/env bash
# scripts/bucket_manager.sh
# Engine de manipulacao do arquivo de bucket para o AetherCLI / MCP
set -eu

# Obtem o diretorio raiz do repositorio
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Resolve o bucketfile:
# 1. Argumento --file / -f
# 2. Variavel de ambiente BUCKETFILE
# 3. Parametro default
DEFAULT_BUCKET="${REPO_ROOT}/done/bucket-0.6.7a-bugfixes.md"
TARGET_FILE="${BUCKETFILE:-$DEFAULT_BUCKET}"

# Funcao de ajuda
usage() {
    cat <<EOF
Uso: bucket_manager.sh <comando> [argumentos]

Comandos:
  board                       Exibe resumo do quadro Kanban no terminal
  list [--status S]           Lista tarefas (todos, todo, doing, done, blocked)
  show <task_id>              Exibe detalhes de uma tarefa especifica
  status <task_id> <status>   Altera o status de uma tarefa ([ ], [/], [!], [X])
  complete <task_id> <test>   Conclui tarefa ([X]) registrando evidencia de teste
  add <cluster> <id> <desc>   Adiciona nova tarefa ao bucket
  count                       Exibe contagem de itens por status
EOF
}

cmd="${1:-board}"
shift || true

# Garante que o arquivo existe
if [[ ! -f "$TARGET_FILE" ]]; then
    echo "%% Erro: Arquivo de bucket nao encontrado: $TARGET_FILE"
    exit 1
fi

case "$cmd" in
    board)
        echo "=========================================================="
        echo "           BASIC NEXT BUCKET KANBAN BOARD                 "
        echo "=========================================================="
        echo "Arquivo: $(basename "$TARGET_FILE")"
        echo ""
        
        TOTAL_TODO=$(grep -E '^[[:space:]]*- \[ \]' "$TARGET_FILE" | wc -l | tr -d ' ')
        TOTAL_DOING=$(grep -E '^[[:space:]]*- \[/\]' "$TARGET_FILE" | wc -l | tr -d ' ')
        TOTAL_BLOCKED=$(grep -E '^[[:space:]]*- \[!\]' "$TARGET_FILE" | wc -l | tr -d ' ')
        TOTAL_DONE=$(grep -E '^[[:space:]]*- \[X\]' "$TARGET_FILE" | wc -l | tr -d ' ')
        
        echo "METRICAS GERAIS:"
        echo "  [ ] A Fazer (TODO):    $TOTAL_TODO"
        echo "  [/] Em Andamento:      $TOTAL_DOING"
        echo "  [!] Bloqueado (Gates): $TOTAL_BLOCKED"
        echo "  [X] Concluido (DONE):  $TOTAL_DONE"
        echo "----------------------------------------------------------"
        echo ""
        echo "ITENS EM ANDAMENTO ([/]):"
        if [[ "$TOTAL_DOING" -eq 0 ]]; then
            echo "  (Nenhum item em andamento no momento)"
        else
            grep -E '^[[:space:]]*- \[/\]' "$TARGET_FILE" | head -n 10 | sed 's/^[[:space:]]*/  /'
        fi
        echo ""
        echo "PROXIMOS ITENS A FAZER ([ ]):"
        grep -E '^[[:space:]]*- \[ \]' "$TARGET_FILE" | head -n 10 | sed 's/^[[:space:]]*/  /'
        echo ""
        echo "=========================================================="
        ;;

    list)
        filter="${1:-all}"
        case "$filter" in
            todo)
                grep -n -E '^[[:space:]]*- \[ \]' "$TARGET_FILE" | sed 's/^[[:space:]]*/  /'
                ;;
            doing)
                grep -n -E '^[[:space:]]*- \[/\]' "$TARGET_FILE" | sed 's/^[[:space:]]*/  /'
                ;;
            blocked)
                grep -n -E '^[[:space:]]*- \[!\]' "$TARGET_FILE" | sed 's/^[[:space:]]*/  /'
                ;;
            done)
                grep -n -E '^[[:space:]]*- \[X\]' "$TARGET_FILE" | sed 's/^[[:space:]]*/  /'
                ;;
            *)
                echo "--- A FAZER ([ ]) ---"
                grep -n -E '^[[:space:]]*- \[ \]' "$TARGET_FILE" | head -n 15
                echo ""
                echo "--- CONCLUIDOS RECENTES ([X]) ---"
                grep -n -E '^[[:space:]]*- \[X\]' "$TARGET_FILE" | tail -n 10
                ;;
        esac
        ;;

    show)
        task_id="${1:-}"
        if [[ -z "$task_id" ]]; then
            echo "%% Erro: task_id obrigatorio para 'show'."
            exit 1
        fi
        
        echo "Detalhes da tarefa: $task_id"
        echo "----------------------------------------------------------"
        # Encontra a linha da tarefa e mostra as linhas de contexto seguintes
        line_num=$(grep -n -E "^[[:space:]]*- \[[ X/!]\] \*\*${task_id}\*\*" "$TARGET_FILE" | cut -d: -f1 | head -n 1 || true)
        if [[ -z "$line_num" ]]; then
            line_num=$(grep -n -E "^[[:space:]]*- \[[ X/!]\] ${task_id}:" "$TARGET_FILE" | cut -d: -f1 | head -n 1 || true)
        fi
        if [[ -z "$line_num" ]]; then
            line_num=$(grep -n -E "\b${task_id}\b" "$TARGET_FILE" | head -n 1 | cut -d: -f1 || true)
        fi

        if [[ -n "$line_num" ]]; then
            sed -n "${line_num},$((line_num + 12))p" "$TARGET_FILE"
        else
            echo "%% Tarefa '$task_id' nao encontrada no arquivo $TARGET_FILE"
            exit 1
        fi
        ;;

    status)
        task_id="${1:-}"
        new_status="${2:-}"
        if [[ -z "$task_id" || -z "$new_status" ]]; then
            echo "%% Erro: uso: status <task_id> <todo|doing|blocked|done>"
            exit 1
        fi

        case "$new_status" in
            todo) mark=" " ;;
            doing) mark="/" ;;
            blocked) mark="!" ;;
            done) mark="X" ;;
            *) echo "%% Erro: status invalido: $new_status (use todo, doing, blocked ou done)"; exit 1 ;;
        esac

        # Atualiza a linha do checkbox no arquivo
        python3 -c '
import sys, re
target_file, task_id, mark = sys.argv[1], sys.argv[2], sys.argv[3]
with open(target_file, "r", encoding="utf-8") as f:
    content = f.read()

pattern = rf"^([ \t]*- \[)[ X/!](] (?:\*\*{re.escape(task_id)}\*\*|{re.escape(task_id)}[: ]))"
new_content, count = re.subn(pattern, rf"\g<1>{mark}\g<2>", content, flags=re.MULTILINE)

if count == 0:
    # Tenta padrão mais permissivo
    pattern2 = rf"^([ \t]*- \[)[ X/!](].*?\b{re.escape(task_id)}\b)"
    new_content, count = re.subn(pattern2, rf"\g<1>{mark}\g<2>", content, count=1, flags=re.MULTILINE)

if count > 0:
    with open(target_file, "w", encoding="utf-8") as f:
        f.write(new_content)
    print(f"% Tarefa {task_id} atualizada com sucesso para [{mark}].")
else:
    print(f"%% Erro: tarefa {task_id} nao encontrada para atualizacao.")
    sys.exit(1)
' "$TARGET_FILE" "$task_id" "$mark"
        ;;

    complete)
        task_id="${1:-}"
        evidence="${2:-}"
        if [[ -z "$task_id" ]]; then
            echo "%% Erro: uso: complete <task_id> [evidencia_teste]"
            exit 1
        fi
        
        today=$(date +"%Y-%m-%d")
        python3 -c '
import sys, re
target_file, task_id, evidence, today = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
with open(target_file, "r", encoding="utf-8") as f:
    content = f.read()

pattern = rf"^([ \t]*- \[)[ /!](] (?:\*\*{re.escape(task_id)}\*\*|{re.escape(task_id)}[: ]))"
note = f" (Verified: {evidence}, {today})" if evidence else f" ({today})"

def repl(m):
    return f"{m.group(1)}X{m.group(2)}"

new_content, count = re.subn(pattern, repl, content, flags=re.MULTILINE)

if count == 0:
    pattern2 = rf"^([ \t]*- \[)[ /!](].*?\b{re.escape(task_id)}\b)"
    new_content, count = re.subn(pattern2, repl, content, count=1, flags=re.MULTILINE)

if count > 0:
    with open(target_file, "w", encoding="utf-8") as f:
        f.write(new_content)
    print(f"% Tarefa {task_id} marcada como CONCLUIDA [X]{note}.")
else:
    print(f"%% Erro: tarefa {task_id} nao encontrada ou ja concluida.")
    sys.exit(1)
' "$TARGET_FILE" "$task_id" "$evidence" "$today"
        ;;

    count)
        TODO=$(grep -E '^[[:space:]]*- \[ \]' "$TARGET_FILE" | wc -l | tr -d ' ')
        DOING=$(grep -E '^[[:space:]]*- \[/\]' "$TARGET_FILE" | wc -l | tr -d ' ')
        BLOCKED=$(grep -E '^[[:space:]]*- \[!\]' "$TARGET_FILE" | wc -l | tr -d ' ')
        DONE=$(grep -E '^[[:space:]]*- \[X\]' "$TARGET_FILE" | wc -l | tr -d ' ')
        echo "TODO: $TODO | DOING: $DOING | BLOCKED: $BLOCKED | DONE: $DONE"
        ;;

    *)
        usage
        exit 1
        ;;
esac
