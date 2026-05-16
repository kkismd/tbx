_tbx_task_completion() {
    local cur prev
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    case "$prev" in
        tbx-task|scripts/tbx-task|tt)
            COMPREPLY=($(compgen -W "current prompt" -- "$cur"))
            return
            ;;
        prompt)
            COMPREPLY=($(compgen -W "implement review fix after-merge" -- "$cur"))
            return
            ;;
    esac
}

complete -F _tbx_task_completion tbx-task
complete -F _tbx_task_completion scripts/tbx-task
